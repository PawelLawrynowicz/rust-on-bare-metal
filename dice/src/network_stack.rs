#![allow(dead_code)]

use alloc::sync::Arc;
pub use drogue_network;
pub use nb;
pub use smoltcp;

use drogue_network::tcp::{Mode, TcpError, TcpImplError};

use smoltcp::socket::AnySocket;
use smoltcp::wire::{IpAddress, IpCidr, Ipv4Address, Ipv4Cidr};
use smoltcp::{dhcp::Dhcpv4Client, socket::SocketSet};

use core::cell::RefCell;
use heapless::Vec;
use nanorand::{wyrand::WyRand, RNG};
use spin::{Mutex, MutexGuard};

// The start of TCP port dynamic range allocation.
const TCP_PORT_DYNAMIC_RANGE_START: u16 = 49152;

#[derive(Debug)]
pub enum NetworkError {
    NoSocket,
    ConnectionFailure,
    SocketNotOpen,
    ReadFailure,
    WriteFailure,
    Unsupported,
    NoIpAddress,
    Timeout,
    Busy,
    Impl(TcpImplError),
}

impl From<TcpError> for NetworkError {
    fn from(item: TcpError) -> Self {
        match item {
            TcpError::NoAvailableSockets => NetworkError::NoSocket,
            TcpError::ConnectionRefused => NetworkError::ConnectionFailure,
            TcpError::SocketNotOpen => NetworkError::SocketNotOpen,
            TcpError::WriteError => NetworkError::WriteFailure,
            TcpError::ReadError => NetworkError::ReadFailure,
            TcpError::Timeout => NetworkError::Timeout,
            TcpError::Busy => NetworkError::Busy,
            TcpError::Impl(impl_error) => NetworkError::Impl(impl_error),
            _ => NetworkError::ConnectionFailure,
        }
    }
}

impl Into<TcpError> for NetworkError {
    fn into(self) -> TcpError {
        match self {
            NetworkError::NoSocket => TcpError::NoAvailableSockets,
            NetworkError::ConnectionFailure => TcpError::ConnectionRefused,
            NetworkError::SocketNotOpen => TcpError::SocketNotOpen,
            NetworkError::ReadFailure => TcpError::ReadError,
            NetworkError::WriteFailure => TcpError::WriteError,
            NetworkError::Unsupported => TcpError::ConnectionRefused,
            NetworkError::NoIpAddress => TcpError::ConnectionRefused,
            NetworkError::Timeout => TcpError::Timeout,
            NetworkError::Busy => TcpError::Busy,
            NetworkError::Impl(impl_error) => TcpError::Impl(impl_error),
        }
    }
}

///! Network abstraction layer for smoltcp.
pub struct NetworkStack<'a, 'b, DeviceT>
where
    DeviceT: for<'c> smoltcp::phy::Device<'c>,
{
    network_interface: Arc<Mutex<smoltcp::iface::EthernetInterface<'b, DeviceT>>>,
    dhcp_client: RefCell<Option<Dhcpv4Client>>,
    sockets: Arc<Mutex<SocketSet<'a>>>,
    used_ports: RefCell<Vec<u16, 16>>,
    unused_handles: RefCell<Vec<smoltcp::socket::SocketHandle, 16>>,
    randomizer: RefCell<WyRand>,
    name_servers: RefCell<[Option<smoltcp::wire::Ipv4Address>; 3]>,
}

impl<'a, 'b, DeviceT> NetworkStack<'a, 'b, DeviceT>
where
    DeviceT: for<'c> smoltcp::phy::Device<'c>,
{
    /// Construct a new network stack.
    ///
    /// # Note
    /// This implementation only supports up to 16 usable sockets.
    ///
    /// Any handles provided to this function must not be used after constructing the network
    /// stack.
    ///
    /// This implementation currently only supports IPv4.
    ///
    /// # Args
    /// * `stack` - The ethernet interface to construct the network stack from.
    /// * `sockets` - The socket set to contain any socket state for the stack.
    /// * `handles` - A list of socket handles that can be used.
    /// * `dhcp` - An optional DHCP client if DHCP usage is desired. If None, DHCP will not be used.
    ///
    /// # Returns
    /// A embedded-nal-compatible network stack.
    pub fn new(
        stack: smoltcp::iface::EthernetInterface<'b, DeviceT>,
        sockets: smoltcp::socket::SocketSet<'a>,
        handles: &[smoltcp::socket::SocketHandle],
        dhcp: Option<Dhcpv4Client>,
    ) -> Self {
        let mut unused_handles: Vec<smoltcp::socket::SocketHandle, 16> = Vec::new();
        for handle in handles.iter() {
            // Note: If the user supplies too many handles, we choose to silently drop them.
            unused_handles.push(*handle).ok();
        }

        NetworkStack {
            network_interface: Arc::new(Mutex::new(stack)),
            sockets: Arc::new(Mutex::new(sockets)),
            used_ports: RefCell::new(Vec::new()),
            randomizer: RefCell::new(WyRand::new_seed(0)),
            dhcp_client: RefCell::new(dhcp),
            unused_handles: RefCell::new(unused_handles),
            name_servers: RefCell::new([None, None, None]),
        }
    }

    pub fn get_socket_set(&mut self) -> Option<MutexGuard<SocketSet<'a>>> {
        self.sockets.try_lock()
    }

    /// Seed the TCP port randomizer.
    ///
    /// # Args
    /// * `seed` - A seed of random data to use for randomizing local TCP port selection.
    pub fn seed_random_port(&mut self, seed: &[u8]) {
        self.randomizer.borrow_mut().reseed(seed)
    }

    /// Poll the network stack for potential updates.
    ///
    /// # Returns
    /// A boolean indicating if the network stack updated in any way.
    pub fn poll(&self, time: u32) -> Result<bool, smoltcp::Error> {
        let sockets = self.sockets.try_lock();
        let interface = self.network_interface.try_lock();

        if sockets.is_none() || interface.is_none() {
            return Ok(false);
        }

        let mut sockets = sockets.unwrap();
        let mut interface = interface.unwrap();

        let now = smoltcp::time::Instant::from_millis(time as i64);
        let updated = match interface.poll(&mut sockets, now) {
            Ok(updated) => updated,
            err => return err,
        };

        // Service the DHCP client.
        if let Some(dhcp_client) = &mut *self.dhcp_client.borrow_mut() {
            match dhcp_client.poll(&mut interface, &mut sockets, now) {
                Ok(Some(config)) => {
                    if let Some(cidr) = config.address {
                        if cidr.address().is_unicast() {
                            // Note(unwrap): This stack only supports IPv4 and the client must have
                            // provided an address.
                            if cidr.address().is_unspecified()
                                || interface.ipv4_address().unwrap() != cidr.address()
                            {
                                // If our address has updated or is not specified, close all
                                // sockets. Note that we have to ensure that the sockets we borrowed
                                // earlier are now returned.
                                drop(sockets);
                                self.close_sockets();

                                interface.update_ip_addrs(|addrs| {
                                    // Note(unwrap): This stack requires at least 1 Ipv4 Address.
                                    let addr = addrs
                                        .iter_mut()
                                        .filter(|cidr| match cidr.address() {
                                            IpAddress::Ipv4(_) => true,
                                            _ => false,
                                        })
                                        .next()
                                        .unwrap();

                                    *addr = IpCidr::Ipv4(cidr);
                                });
                            }
                        }
                    }

                    // Store DNS server addresses for later read-back
                    *self.name_servers.borrow_mut() = config.dns_servers;

                    if let Some(route) = config.router {
                        // Note: If the user did not provide enough route storage, we may not be
                        // able to store the gateway.
                        interface.routes_mut().add_default_ipv4_route(route)?;
                    }
                }
                Ok(None) => {}
                Err(err) => return Err(err),
            }
        }

        Ok(updated)
    }

    /// Force-close all sockets.
    pub fn close_sockets(&self) {
        // Close all sockets.
        let sockets = self.sockets.try_lock();

        if sockets.is_none() {
            return;
        }

        let mut sockets = sockets.unwrap();

        for mut socket in sockets.iter_mut() {
            // We only explicitly can close TCP sockets because we cannot access other socket types.
            if let Some(ref mut socket) =
                smoltcp::socket::TcpSocket::downcast(smoltcp::socket::SocketRef::new(&mut socket))
            {
                socket.abort();
            }
        }
    }

    /// Handle a disconnection of the physical interface.
    pub fn handle_link_reset(&mut self) {
        // Reset the DHCP client.
        if let Some(ref mut client) = *self.dhcp_client.borrow_mut() {
            client.reset(smoltcp::time::Instant::from_millis(-1));
        }

        // Close all of the sockets and de-configure the interface.
        self.close_sockets();

        let interface = self.network_interface.try_lock();

        if interface.is_none(){
            return;
        }

        let mut interface = interface.unwrap();

        interface.update_ip_addrs(|addrs| {
            addrs.iter_mut().next().map(|addr| {
                *addr = IpCidr::Ipv4(Ipv4Cidr::new(Ipv4Address::UNSPECIFIED, 0));
            });
        });
    }

    // Get an ephemeral TCP port number.
    fn get_ephemeral_port(&self) -> u16 {
        loop {
            // Get the next ephemeral port by generating a random, valid TCP port continuously
            // until an unused port is found.
            let random_offset = {
                let random_data = self.randomizer.borrow_mut().rand();
                u16::from_be_bytes([random_data[0], random_data[1]])
            };

            let port = TCP_PORT_DYNAMIC_RANGE_START
                + random_offset % (u16::MAX - TCP_PORT_DYNAMIC_RANGE_START);
            if self
                .used_ports
                .borrow()
                .iter()
                .find(|&x| *x == port)
                .is_none()
            {
                return port;
            }
        }
    }

    pub fn is_ip_unspecified(&self) -> bool {
        // Note(unwrap): This stack only supports Ipv4.
        self.network_interface
            .lock()
            .ipv4_addr()
            .unwrap()
            .is_unspecified()
    }
}

impl<'a, 'b, DeviceT> drogue_network::tcp::TcpStack for NetworkStack<'a, 'b, DeviceT>
where
    DeviceT: for<'c> smoltcp::phy::Device<'c>,
{
    type Error = NetworkError;
    type TcpSocket = smoltcp::socket::SocketHandle;

    fn open(
        &self,
        mode: drogue_network::tcp::Mode,
    ) -> Result<smoltcp::socket::SocketHandle, NetworkError> {
        // If we do not have a valid IP address yet, do not open the socket.
        if self.is_ip_unspecified() {
            return Err(NetworkError::NoIpAddress);
        }

        match mode {
            Mode::NonBlocking => {}
            _ => unimplemented!(),
        }

        match self.unused_handles.borrow_mut().pop() {
            Some(handle) => {
                // Abort any active connections on the handle.
                let sockets = self.sockets.try_lock();

                if sockets.is_none(){
                    return Err(NetworkError::Busy);
                }

                let mut sockets = sockets.unwrap();

                let internal_socket: &mut smoltcp::socket::TcpSocket = &mut *sockets.get(handle);
                internal_socket.abort();

                Ok(handle)
            }
            None => Err(NetworkError::NoSocket),
        }
    }

    fn connect(
        &self,
        socket: smoltcp::socket::SocketHandle,
        remote: drogue_network::addr::HostSocketAddr,
    ) -> Result<smoltcp::socket::SocketHandle, NetworkError> {
        // If there is no longer an IP address assigned to the interface, do not allow usage of the
        // socket.
        if self.is_ip_unspecified() {
            self.close(socket)?;
            return Err(NetworkError::NoIpAddress);
        }

        let mut sockets = self.sockets.lock();
        let internal_socket: &mut smoltcp::socket::TcpSocket = &mut *sockets.get(socket);

        // If we're already in the process of connecting, ignore the request silently.
        if internal_socket.is_open() {
            return Ok(socket);
        }

        match remote.addr().ip() {
            drogue_network::addr::IpAddr::V4(addr) => {
                let octets = addr.octets();
                let address =
                    smoltcp::wire::Ipv4Address::new(octets[0], octets[1], octets[2], octets[3]);

                // Note(unwrap): Only one port is allowed per socket, so this push should never
                // fail.
                let local_port = self.get_ephemeral_port();
                self.used_ports.borrow_mut().push(local_port).unwrap();

                internal_socket
                    .connect((address, remote.port()), local_port)
                    .or_else(|_| {
                        self.close(socket)?;
                        Err(NetworkError::ConnectionFailure)
                    })?;
                Ok(socket)
            }

            // We only support IPv4.
            _ => Err(NetworkError::Unsupported),
        }
    }

    fn is_connected(&self, socket: &smoltcp::socket::SocketHandle) -> Result<bool, NetworkError> {
        // If there is no longer an IP address assigned to the interface, do not allow usage of the
        // socket.
        if self.is_ip_unspecified() {
            return Err(NetworkError::NoIpAddress);
        }

        let mut sockets = self.sockets.lock();
        let socket: &mut smoltcp::socket::TcpSocket = &mut *sockets.get(*socket);
        Ok(socket.may_send() && socket.may_recv())
    }

    fn write(
        &self,
        socket: &mut smoltcp::socket::SocketHandle,
        buffer: &[u8],
    ) -> Result<usize, nb::Error<NetworkError>> {
        // If there is no longer an IP address assigned to the interface, do not allow usage of the
        // socket.

        if self.is_ip_unspecified() {
            return Err(nb::Error::Other(NetworkError::NoIpAddress));
        }

        let sockets = self.sockets.try_lock();

        if sockets.is_none(){
            return Err(nb::Error::WouldBlock);
        }

        let mut sockets = sockets.unwrap();

        let socket: &mut smoltcp::socket::TcpSocket = &mut *sockets.get(*socket);

        if !socket.is_active() {
            return Err(nb::Error::Other(NetworkError::SocketNotOpen));
        }

        if !socket.can_send() {
            return Err(nb::Error::WouldBlock);
        }

        socket
            .send_slice(buffer)
            .map_err(|_| nb::Error::Other(NetworkError::WriteFailure))
    }

    fn read(
        &self,
        socket: &mut smoltcp::socket::SocketHandle,
        buffer: &mut [u8],
    ) -> nb::Result<usize, NetworkError> {
        // If there is no longer an IP address assigned to the interface, do not allow usage of the
        // socket.
        if self.is_ip_unspecified() {
            return Err(nb::Error::Other(NetworkError::NoIpAddress));
        }

        let sockets = self.sockets.try_lock();

        if sockets.is_none(){
            return Err(nb::Error::WouldBlock);
        }

        let mut sockets = sockets.unwrap();

        let socket: &mut smoltcp::socket::TcpSocket = &mut *sockets.get(*socket);

        if !socket.is_open() {
            return Err(nb::Error::Other(NetworkError::SocketNotOpen));
        }

        if !socket.can_recv() {
            return Err(nb::Error::WouldBlock);
        }

        socket
            .recv_slice(buffer)
            .map_err(|_| nb::Error::Other(NetworkError::ReadFailure))
    }

    fn close(&self, socket: smoltcp::socket::SocketHandle) -> Result<(), NetworkError> {
        let mut sockets = self.sockets.lock();
        let internal_socket: &mut smoltcp::socket::TcpSocket = &mut *sockets.get(socket);

        // Remove the bound port from the used_ports buffer.
        let local_port = internal_socket.local_endpoint().port;
        let mut used_ports = self.used_ports.borrow_mut();

        //TODO: Describe the problem
        if local_port != 0 {
            let index = used_ports
                .iter()
                .position(|&port| port == local_port)
                .unwrap();
            used_ports.swap_remove(index);
        }

        internal_socket.close();
        self.unused_handles.borrow_mut().push(socket).unwrap();
        Ok(())
    }
}
