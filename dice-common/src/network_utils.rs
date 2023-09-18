use smoltcp::iface::{
    EthernetInterface, EthernetInterfaceBuilder, Neighbor, NeighborCache, Route, Routes,
};
use smoltcp::phy::Device;
use smoltcp::socket::IcmpPacketMetadata;
use smoltcp::socket::{
    IcmpSocket, IcmpSocketBuffer, RawPacketMetadata, RawSocket, RawSocketBuffer,
};
use smoltcp::wire::{EthernetAddress, IpAddress, IpCidr, IpProtocol, IpVersion, Ipv4Address};

/// A struct used that holds all buffers required by a raw socket.
/// Useful when you can't use dynamic allocation.
pub struct RawSocketStorage<'a> {
    pub tx_metadata: [RawPacketMetadata; 1],
    pub rx_metadata: [RawPacketMetadata; 1],
    pub tx_buffer: &'a mut [u8],
    pub rx_buffer: &'a mut [u8],
}

/// Creates an IP raw socket
/// # Arguments
/// * `socket storage` - A mutable reference to RawSocketStorage instance.
/// * `ip_version` - Ip Version to bind the socket to.
/// * `protocol` - A protocol to bind the socket to.
pub fn create_raw_socket<'a>(
    socket_storage: &'a mut RawSocketStorage,
    ip_version: IpVersion,
    protocol: IpProtocol,
) -> RawSocket<'a> {
    let raw_tx_buffer = RawSocketBuffer::new(
        &mut socket_storage.tx_metadata[..],
        &mut socket_storage.tx_buffer[..],
    );
    let raw_rx_buffer = RawSocketBuffer::new(
        &mut socket_storage.rx_metadata[..],
        &mut socket_storage.rx_buffer[..],
    );

    let raw_socket = RawSocket::new(ip_version, protocol, raw_rx_buffer, raw_tx_buffer);

    raw_socket
}

/// A struct used that holds all buffers required by an EthernetInterface.
/// Useful when you can't use dynamic allocation.
pub struct EthernetInterfaceStorage {
    pub neighbor_storage: [Option<(IpAddress, Neighbor)>; 8],
    pub routes_storage: [Option<(IpCidr, Route)>; 1],
    pub ip_storage: [IpCidr; 1],
}

impl EthernetInterfaceStorage {
    ///Returns a new instance of EthernetInterfaceStorage
    pub fn new() -> Self {
        EthernetInterfaceStorage {
            neighbor_storage: [None; 8],
            routes_storage: [None],
            ip_storage: [IpCidr::new(IpAddress::v4(127, 0, 0, 1), 24)],
        }
    }
}

/// Creates new EthernetInterface. Note that interface created by this function only supports IPv4.
/// # Arguments
/// * `device` - An ethernet device. Must implement smoltcp Device trait.
/// * `ethernet_addr` - Ethernet (MAC) address of the device
/// * `ip_addr` - IPv4 address of the device
/// * `mask` - Network mask in number of bytes
/// * `default_gw_addr` - Default gateway IPv4 address
/// * `storage` - Mutable reference to EthernetInterfaceStorage instance
pub fn create_ethernet_iface<DeviceT>(
    device: DeviceT,
    ethernet_addr: EthernetAddress,
    ip_addr: IpAddress,
    mask: u8,
    default_gw_addr: Ipv4Address,
    storage: &mut EthernetInterfaceStorage,
) -> EthernetInterface<DeviceT>
where
    DeviceT: for<'d> Device<'d>,
{
    storage.ip_storage[0] = IpCidr::new(ip_addr, mask);

    let mut routes = Routes::new(&mut storage.routes_storage[..]);
    routes.add_default_ipv4_route(default_gw_addr).unwrap();

    let neighbor_cache = NeighborCache::new(&mut storage.neighbor_storage[..]);

    let iface = EthernetInterfaceBuilder::new(device)
        .ethernet_addr(ethernet_addr)
        .ip_addrs(&mut storage.ip_storage[..])
        .routes(routes)
        .neighbor_cache(neighbor_cache)
        .finalize();

    iface
}

/// A struct used that holds all buffers required by an IcmpSocket.
/// Useful when you can't use dynamic allocation.
pub struct IcmpSocketStorage<'a> {
    pub tx_metadata: [IcmpPacketMetadata; 1],
    pub rx_metadata: [IcmpPacketMetadata; 1],
    pub tx_buffer: &'a mut [u8],
    pub rx_buffer: &'a mut [u8],
}

/// Creates new ICMP socket
/// # Arguments
/// * `socket_storage` - a mutable reference to IcmpSocketStorage instance
pub fn create_icmp_socket<'a>(socket_storage: &'a mut IcmpSocketStorage) -> IcmpSocket<'a> {
    let icmp_tx_buffer = IcmpSocketBuffer::new(
        &mut socket_storage.tx_metadata[..],
        &mut socket_storage.tx_buffer[..],
    );
    let icmp_rx_buffer = IcmpSocketBuffer::new(
        &mut socket_storage.rx_metadata[..],
        &mut socket_storage.rx_buffer[..],
    );

    let icmp_socket = IcmpSocket::new(icmp_rx_buffer, icmp_tx_buffer);

    icmp_socket
}

#[cfg(test)]
mod tests {
    use super::*;
    //unfortunately, we can't use loopback for tests because we can't use alloc feature :(
    //use smoltcp::phy::Loopback;

    use crate::mock_ethernet::StmPhy;
    use smoltcp::wire::Ipv6Cidr;
    pub const SOCKET_BUFFER_SIZE: usize = 1500;

    #[test]
    fn create_socket_test() {
        let mut tx_b: [u8; SOCKET_BUFFER_SIZE] = [0; SOCKET_BUFFER_SIZE];
        let mut rx_b: [u8; SOCKET_BUFFER_SIZE] = [0; SOCKET_BUFFER_SIZE];

        let mut s_storage = RawSocketStorage {
            tx_metadata: [RawPacketMetadata::EMPTY],
            rx_metadata: [RawPacketMetadata::EMPTY],
            tx_buffer: &mut tx_b,
            rx_buffer: &mut rx_b,
        };

        let socket = create_raw_socket(&mut s_storage, IpVersion::Ipv4, IpProtocol::Icmp);

        assert_eq!(socket.ip_version(), IpVersion::Ipv4);
        assert_eq!(socket.ip_protocol(), IpProtocol::Icmp);
        assert_eq!(socket.payload_recv_capacity(), SOCKET_BUFFER_SIZE);
        assert_eq!(socket.payload_send_capacity(), SOCKET_BUFFER_SIZE);
    }

    #[test]
    fn create_icmp_socket_test() {
        let mut tx_b: [u8; SOCKET_BUFFER_SIZE] = [0; SOCKET_BUFFER_SIZE];
        let mut rx_b: [u8; SOCKET_BUFFER_SIZE] = [0; SOCKET_BUFFER_SIZE];

        let mut s_storage = IcmpSocketStorage {
            tx_metadata: [IcmpPacketMetadata::EMPTY],
            rx_metadata: [IcmpPacketMetadata::EMPTY],
            tx_buffer: &mut tx_b,
            rx_buffer: &mut rx_b,
        };

        let socket = create_icmp_socket(&mut s_storage);

        assert_eq!(socket.payload_recv_capacity(), SOCKET_BUFFER_SIZE);
        assert_eq!(socket.payload_send_capacity(), SOCKET_BUFFER_SIZE);
    }

    #[test]
    fn create_iface_test() {
        let ip_address = IpAddress::v4(192, 168, 1, 2);
        let gw_address = Ipv4Address::new(192, 168, 1, 1);

        let device = StmPhy::new();
        let mut i_storage = EthernetInterfaceStorage {
            ip_storage: [IpCidr::Ipv6(Ipv6Cidr::SOLICITED_NODE_PREFIX)],
            neighbor_storage: [None; 8],
            routes_storage: [None],
        };

        let iface = create_ethernet_iface(
            device,
            EthernetAddress::default(),
            ip_address,
            24,
            gw_address,
            &mut i_storage,
        );

        assert_eq!(iface.ethernet_addr(), EthernetAddress::default());
        assert_eq!(
            iface.ipv4_address().unwrap(),
            Ipv4Address::new(192, 168, 1, 2)
        );
    }
}
