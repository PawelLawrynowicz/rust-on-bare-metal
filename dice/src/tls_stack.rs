#![allow(dead_code)]

use core::{alloc::Layout, cell::RefCell, convert::TryInto, mem::size_of, slice};

extern crate alloc;

use alloc::sync::Arc;

use drogue_network::{
    addr::HostSocketAddr,
    tcp::{Mode, TcpError, TcpStack},
};

use drogue_tls::entropy::EntropySource;
use drogue_tls_sys::types::{c_char, c_int, c_uchar, c_void};
use drogue_tls_sys::*;

use nb::Error;

use spin::Mutex;

pub enum TlsState {
    BeforeInit,
    NotConnected,
    Connected,
}

#[derive(Debug)]
pub enum TlsError {
    CannotConnect,
    CannotRead,
    CannotWrite,
    Timeout,
    Busy,
}

impl From<TcpError> for TlsError {
    fn from(item: TcpError) -> Self {
        match item {
            TcpError::NoAvailableSockets => TlsError::CannotConnect,
            TcpError::ConnectionRefused => TlsError::CannotConnect,
            TcpError::SocketNotOpen => TlsError::CannotConnect,
            TcpError::WriteError => TlsError::CannotWrite,
            TcpError::ReadError => TlsError::CannotRead,
            TcpError::Timeout => TlsError::Timeout,
            TcpError::Busy => TlsError::Busy,
            TcpError::Impl(_impl_error) => TlsError::CannotConnect,
            _ => TlsError::CannotConnect,
        }
    }
}

impl Into<TcpError> for TlsError {
    fn into(self) -> TcpError {
        match self {
            TlsError::CannotConnect => TcpError::ConnectionRefused,
            TlsError::CannotRead => TcpError::ReadError,
            TlsError::CannotWrite => TcpError::WriteError,
            TlsError::Timeout => TcpError::Timeout,
            TlsError::Busy => TcpError::Busy,
        }
    }
}

#[repr(C)]
pub struct TlsLayer<'a, StackT: TcpStack> {
    stack: &'a mut StackT,
    state: Arc<Mutex<TlsState>>,
    socket: RefCell<Option<StackT::TcpSocket>>,
    entropy_context: entropy_context,
    ssl_config: ssl_config,
    ssl_context: ssl_context,
    drbg_ctx: ctr_drbg_context,
}

impl<'a, StackT: TcpStack> TlsLayer<'a, StackT> {
    const PERS: &'static str = "ssl_client1";

    pub fn new(stack: &'a mut StackT) -> Self {
        TlsLayer {
            stack,
            state: Arc::new(Mutex::new(TlsState::BeforeInit)),
            entropy_context: entropy_context::default(),
            drbg_ctx: ctr_drbg_context::default(),
            socket: RefCell::new(None),
            ssl_config: ssl_config::default(),
            ssl_context: ssl_context::default(),
        }
    }

    pub fn init<T: EntropySource>(&mut self, entropy: T) {
        unsafe {
            platform_set_calloc_free(Some(platform_calloc_f), Some(platform_free_f));

            ssl_init(&mut self.ssl_context as *mut _);

            entropy_init(&mut self.entropy_context as *mut _);

            let result = entropy_add_source(
                &mut self.entropy_context as *mut _,
                Some(entropy.get_f()),
                0 as *mut c_void,
                0,
                ENTROPY_SOURCE_STRONG.try_into().unwrap(),
            );

            if result != 0 {
                panic!("Failed to initialize mbedtls!")
            }

            ctr_drbg_init(&mut self.drbg_ctx as *mut _);

            let result = ctr_drbg_seed(
                &mut self.drbg_ctx as *mut _,
                Some(entropy_func),
                &mut self.entropy_context as *mut _ as *mut c_void,
                Self::PERS.as_ptr() as *const c_uchar,
                Self::PERS.len(),
            );

            if result != 0 {
                panic!("Failed to initialize mbedtls!")
            }

            let config_ptr = &mut self.ssl_config as *mut ssl_config;

            ssl_config_init(config_ptr);
            let result = ssl_config_defaults(
                config_ptr,
                SSL_IS_CLIENT.try_into().unwrap(),
                SSL_TRANSPORT_STREAM.try_into().unwrap(),
                SSL_PRESET_DEFAULT.try_into().unwrap(),
            );
            if result != 0 {
                panic!("Failed to initialize mbedtls!")
            }

            ssl_conf_authmode(config_ptr, SSL_VERIFY_NONE.try_into().unwrap());

            ssl_conf_rng(
                config_ptr,
                Some(ctr_drbg_random),
                &mut self.drbg_ctx as *mut _ as *mut c_void,
            );

            let result = ssl_setup(
                &mut self.ssl_context as *mut _,
                &mut self.ssl_config as *mut _,
            );
            if result != 0 {
                panic!("Failed to initialize mbedtls!")
            }

            ssl_set_bio(
                &mut self.ssl_context as *mut _,
                self as *mut _ as *mut c_void,
                Some(send_f::<StackT>),
                Some(recv_f::<StackT>),
                None,
            );

            *self.state.lock() = TlsState::NotConnected;
        }
    }

    pub fn free_tls(&mut self) {
        unsafe {
            ssl_free(&mut self.ssl_context as *mut _);
            ssl_config_free(&mut self.ssl_config as *mut _);
            ctr_drbg_free(&mut self.drbg_ctx as *mut _);
            entropy_free(&mut self.entropy_context as *mut _);
        }
    }

    pub fn handle_disconnected(&mut self) {
        let state = self.state.try_lock();

        if state.is_none() {
            return;
        }

        let mut state = state.unwrap();

        if let TlsState::NotConnected = *state {
            return;
        }

        unsafe {
            ssl_session_reset(&mut self.ssl_context as *mut _);
        }
        self.stack.close(self.socket.take().unwrap()).unwrap();
        *state = TlsState::NotConnected;
    }
}

impl<'a, StackT: TcpStack> TcpStack for TlsLayer<'a, StackT> {
    type TcpSocket = u8;
    type Error = TlsError;

    fn open(&self, mode: Mode) -> Result<Self::TcpSocket, Self::Error> {
        let state = self.state.lock();
        if let TlsState::BeforeInit = *state {
            panic!("TlsLayer must be initialized before trying to open socket!");
        }

        let socket = self
            .stack
            .open(mode)
            .map_err(|_e| TlsError::CannotConnect)?;

        *self.socket.borrow_mut() = Some(socket);

        Ok(0)
    }

    fn connect(
        &self,
        _socket: Self::TcpSocket,
        remote: HostSocketAddr,
    ) -> Result<Self::TcpSocket, Self::Error> {
        let mut state = self.state.lock();

        if let TlsState::BeforeInit = *state {
            panic!("TlsLayer must be initialized before trying to connect!");
        }

        let socket = self
            .stack
            .connect(self.socket.take().unwrap(), remote)
            .map_err(|_e| TlsError::CannotConnect)?;

        *state = TlsState::Connected;

        *self.socket.borrow_mut() = Some(socket);

        Ok(0)
    }

    fn write(
        &self,
        _socket: &mut Self::TcpSocket,
        buffer: &[u8],
    ) -> Result<usize, nb::Error<Self::Error>> {
        let mut state = self.state.lock();
        let mut timeout_counter = 0;
        const TIMEOUT_TRESHOLD: i32 = 100000;

        match *state {
            TlsState::Connected => unsafe {
                let mut len = buffer.len();
                let mut offset: usize = 0;

                loop {
                    if timeout_counter >= TIMEOUT_TRESHOLD {
                        ssl_session_reset(&self.ssl_context as *const _ as *mut _);
                        self.stack.close(self.socket.take().unwrap()).unwrap();
                        *state = TlsState::NotConnected;
                        return Err(Error::Other(TlsError::CannotWrite));
                    }

                    let ret = ssl_write(
                        &self.ssl_context as *const _ as *mut _,
                        buffer.as_ptr().offset(offset as isize),
                        len,
                    );

                    if ret >= 0 {
                        len -= ret as usize;
                        offset += ret as usize;

                        //handle partial write case - call function again
                        if len > 0 {
                            continue;
                        }

                        break;
                    } else {
                        match ret {
                            ERR_SSL_WANT_READ
                            | ERR_SSL_WANT_WRITE
                            | ERR_SSL_ASYNC_IN_PROGRESS
                            | ERR_SSL_CRYPTO_IN_PROGRESS => {
                                timeout_counter += 1;
                                continue;
                            }
                            _ => {
                                //Some error occured, context is now invalid and the connection must be reset.
                                ssl_session_reset(&self.ssl_context as *const _ as *mut _);
                                self.stack.close(self.socket.take().unwrap()).unwrap();
                                *state = TlsState::NotConnected;
                                return Err(Error::Other(TlsError::CannotWrite));
                            }
                        }
                    }
                }
                Ok(offset)
            },
            _ => Err(nb::Error::Other(TlsError::CannotWrite)),
        }
    }

    fn read(
        &self,
        _socket: &mut Self::TcpSocket,
        buffer: &mut [u8],
    ) -> Result<usize, nb::Error<Self::Error>> {
        let mut state = self.state.lock();
        let mut timeout_counter = 0;
        const TIMEOUT_TRESHOLD: i32 = 100000;

        let mut bytes_read: usize = 0;

        match *state {
            TlsState::Connected => unsafe {
                loop {
                    if timeout_counter >= TIMEOUT_TRESHOLD {
                        ssl_session_reset(&self.ssl_context as *const _ as *mut _);
                        self.stack.close(self.socket.take().unwrap()).unwrap();
                        *state = TlsState::NotConnected;
                        return Err(Error::Other(TlsError::CannotRead));
                    }

                    let ret = ssl_read(
                        &self.ssl_context as *const _ as *mut _,
                        buffer.as_mut_ptr() as *mut _,
                        buffer.len(),
                    );

                    if ret > 0 {
                        bytes_read += ret as usize;
                        break;
                    }

                    match ret {
                        ERR_SSL_WANT_READ
                        | ERR_SSL_WANT_WRITE
                        | ERR_SSL_ASYNC_IN_PROGRESS
                        | ERR_SSL_CRYPTO_IN_PROGRESS => {
                            timeout_counter += 1;
                            continue;
                        }
                        0 => {
                            ssl_session_reset(&self.ssl_context as *const _ as *mut _);
                            self.stack.close(self.socket.take().unwrap()).unwrap();
                            *state = TlsState::NotConnected;
                            return Ok(0);
                        }
                        _ => {
                            ssl_session_reset(&self.ssl_context as *const _ as *mut _);
                            self.stack.close(self.socket.take().unwrap()).unwrap();
                            *state = TlsState::NotConnected;
                            return Err(Error::Other(TlsError::CannotRead));
                        }
                    }
                }

                Ok(bytes_read)
            },
            _ => Err(nb::Error::Other(TlsError::CannotRead)),
        }
    }

    fn close(&self, _socket: Self::TcpSocket) -> Result<(), Self::Error> {
        let mut state = self.state.lock();
        match *state {
            TlsState::Connected => {
                unsafe {
                    let _result = ssl_close_notify(&self.ssl_context as *const _ as *mut _);
                }
                self.stack.close(self.socket.take().unwrap()).unwrap();
                *state = TlsState::NotConnected;
            }
            _ => {}
        };

        Ok(())
    }

    fn is_connected(&self, _socket: &Self::TcpSocket) -> Result<bool, Self::Error> {
        match *self.state.lock() {
            TlsState::NotConnected => Ok(false),
            TlsState::BeforeInit => Ok(false),
            _ => Ok(true),
        }
    }
}

extern "C" fn send_f<StackT: TcpStack>(ctx: *mut c_void, buf: *const c_uchar, len: usize) -> c_int {
    unsafe {
        let layer = &mut *(ctx as *mut TlsLayer<StackT>);

        let slice = slice::from_raw_parts(buf, len);

        let result = layer
            .stack
            .write(layer.socket.borrow_mut().as_mut().unwrap(), slice);

        match result {
            Ok(len) => len as c_int,
            Err(e) => match e {
                Error::WouldBlock => ERR_SSL_WANT_WRITE,
                Error::Other(_) => -1 as c_int,
            },
        }
    }
}

extern "C" fn recv_f<StackT: TcpStack>(ctx: *mut c_void, buf: *mut c_uchar, len: usize) -> c_int {
    unsafe {
        let layer = &mut *(ctx as *mut TlsLayer<StackT>);

        let slice = slice::from_raw_parts_mut(buf, len);

        let result = layer
            .stack
            .read(layer.socket.borrow_mut().as_mut().unwrap(), slice);

        match result {
            Ok(len) => len as c_int,
            Err(e) => match e {
                Error::WouldBlock => ERR_SSL_WANT_READ,
                Error::Other(_) => -1 as c_int,
            },
        }
    }
}

pub extern "C" fn strlen(p: *const c_char) -> usize {
    let mut n = 0;
    unsafe {
        while *p.add(n) != 0 {
            n += 1;
        }
    }
    n
}

extern "C" fn platform_calloc_f(count: usize, size: usize) -> *mut c_void {
    let requested_size = count * size;
    let header_size = 2 * size_of::<usize>();
    let total_size = header_size + requested_size;
    let layout = Layout::from_size_align(total_size, 4)
        .unwrap()
        .pad_to_align();

    unsafe {
        let mut ptr = alloc::alloc::alloc(layout) as *mut usize;

        if ptr.is_null() {
            panic!("Failed to allocate memory!");
        }

        *ptr = layout.size();
        ptr = ptr.add(1);
        *ptr = layout.align();
        ptr = ptr.add(1);
        let mut zeroing = ptr as *mut u8;
        for _ in 0..requested_size {
            zeroing.write(0);
            zeroing = zeroing.add(1);
        }
        ptr as *mut c_void
    }
}

extern "C" fn platform_free_f(ptr: *mut c_void) {
    if ptr as u32 == 0 {
        return;
    }
    unsafe {
        let mut ptr = ptr as *mut usize;
        ptr = ptr.offset(-1);
        let align = *ptr;
        ptr = ptr.offset(-1);
        let size = *ptr;
        alloc::alloc::dealloc(
            ptr as *mut u8,
            Layout::from_size_align(size, align).unwrap(),
        );
    }
}

extern "C" {
    fn platform_snprintf(s: *mut c_char, n: usize, format: *const c_char, ...) -> c_int;
}
