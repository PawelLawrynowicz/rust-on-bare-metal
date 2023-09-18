#![no_std]

//! A very simple HTTP server designed to work in no_std environments.
//! Uses smoltcp TCP sockets for communication.
//! Currently only supports HEAD, GET and POST requests.
//! Currently parsing HTTP headers is not implemented and they're ignored.

use spin::MutexGuard;

use heapless::{FnvIndexMap, String};
use httparse::{self, Header, EMPTY_HEADER};
use smoltcp::wire::{IpAddress, IpEndpoint};
use smoltcp::{
    socket::{SocketHandle, SocketRef, SocketSet, TcpSocket},
    time::Duration,
};

pub use httparse::Request;

pub mod default_pages;
pub mod response;

pub enum ServerError {
    RouteCapacityExceeded,
}

pub struct HttpServer<
    const URL_SIZE: usize,
    const RESPONSE_SIZE: usize,
    const ROUTE_CAPACITY: usize,
    const RX_BUFFER_SIZE: usize,
    const HEADER_BUFFER_LENGTH: usize,
> {
    rx_buffer: [u8; RX_BUFFER_SIZE],
    socket_handle: SocketHandle,
    endpoint: IpEndpoint,
    routes: FnvIndexMap<
        (String<8>, String<URL_SIZE>),
        fn(Request, &[u8]) -> String<RESPONSE_SIZE>,
        ROUTE_CAPACITY,
    >,
    timeout_counter: u32,
}

impl<
        const URL_SIZE: usize,
        const RESPONSE_SIZE: usize,
        const ROUTE_CAPACITY: usize,
        const RX_BUFFER_SIZE: usize,
        const HEADER_BUFFER_LENGTH: usize,
    > HttpServer<URL_SIZE, RESPONSE_SIZE, ROUTE_CAPACITY, RX_BUFFER_SIZE, HEADER_BUFFER_LENGTH>
{
    /// Create a new HTTP server.
    /// # Arguments
    /// * `socket_handle` - A handle to the TCP socket the server will use
    /// * `port` - Port on which the socket will listen for connections
    pub fn new(socket_handle: SocketHandle, port: u16) -> Self {
        let endpoint = IpEndpoint::new(IpAddress::v4(0, 0, 0, 0), port);
        let routes = FnvIndexMap::<
            (String<8>, String<URL_SIZE>),
            fn(Request, &[u8]) -> String<RESPONSE_SIZE>,
            ROUTE_CAPACITY,
        >::new();

        HttpServer {
            rx_buffer: [0; RX_BUFFER_SIZE],
            socket_handle,
            endpoint,
            routes,
            timeout_counter: 0,
        }
    }

    /// Poll the server. It should be called periodically.
    /// This function will perform the following steps:
    /// - Checks if socket is closed. If it is, it is opened and starts listening.
    /// - Checks if there is data in receive buffer of the socket.
    /// - If there's data in socket's rx buffer, it is received into server's buffer and parsed.
    ///   If the parse is successfull, a http response is sent and the connection is closed.
    ///   If any errors occur, the connection is reset
    /// # Arguments:
    /// * `socket_set` - A mutable reference to smoltcp::SocketSet containing the socket the server uses
    pub fn poll(&mut self, socket_set: &mut MutexGuard<SocketSet>) {
        let mut socket = socket_set.get::<TcpSocket>(self.socket_handle);

        if !socket.is_open() {
            socket.set_timeout(Some(Duration::from_secs(2)));
            socket.set_keep_alive(Some(Duration::from_secs(3)));
            socket.listen(self.endpoint).unwrap();
        }

        if socket.may_recv() {
            match socket.recv_slice(&mut self.rx_buffer) {
                Ok(_) => {
                    self.handle_request(&mut socket);
                }
                Err(_) => {
                    socket.close();
                }
            }
            //clear rx buffer
            self.rx_buffer.fill(0);
        }

        if socket.may_send() {
            socket.close();
            self.timeout_counter = 0;
        }

        //Sometimes the socket will freeze in SynReceived state. Using timeout and keep-alive doesn't solve the problem
        //so we need to get a little bit creative.
        if socket.is_active() && !socket.may_recv() && !socket.may_send() {
            self.timeout_counter += 1;

            if self.timeout_counter >= 5000 {
                self.timeout_counter = 0;
                socket.abort();
            }
        }
    }

    /// Add a new route
    /// Currently only custom GET and POST routes are supported.
    /// The route handler receives parsed HTTP request in `request` parameter and request's body in `body` parameter.
    /// The route handler is expected to return a valid Http response in form of a heapless::String
    /// # Arguments
    /// * `method` - Request method in string format. Currently only "GET" and "POST" are supported
    /// * `path` - Route path. Examples: "/", "/resource", "/foo/bar"
    /// * `handler` - A pointer to a function that implements the route handler.
    pub fn add_route(
        &mut self,
        method: &str,
        path: &str,
        handler: fn(Request, &[u8]) -> String<RESPONSE_SIZE>,
    ) -> Result<(), ServerError> {
        assert!(method == "GET" || method == "POST");

        match self
            .routes
            .insert((String::from(method), String::from(path)), handler)
        {
            Ok(_) => Ok(()),
            Err(_) => Err(ServerError::RouteCapacityExceeded),
        }
    }

    fn handle_request(&mut self, socket: &mut SocketRef<TcpSocket>) {
        let mut request_headers = [EMPTY_HEADER; HEADER_BUFFER_LENGTH];
        let parse_result = parse_request(&mut self.rx_buffer, &mut request_headers);

        match parse_result {
            Ok((request, body)) => {
                let method = String::from(request.method.unwrap());
                let path = String::from(request.path.unwrap());

                let response = match method.as_str() {
                    "GET" | "POST" => {
                        let handler = self.routes.get(&(method, path));
                        match handler {
                            Some(handle) => handle(request, body),
                            None => response::not_found_response(),
                        }
                    }
                    //Currently we don't return any headers so HEAD response will be the same every time
                    "HEAD" => {
                        let handler = self.routes.get(&(String::from("GET"), path));
                        match handler {
                            Some(_handle) => response::ok_response(""),
                            None => response::not_found_no_body_response(),
                        }
                    }
                    _ => unsupported_request_handler(request),
                };

                let bytes = response.as_bytes();
                let result = socket.send_slice(bytes);
                match result {
                    Ok(_) => {}
                    Err(_) => {
                        //For some reason, we couldn't send a response. Close connection
                        socket.close();
                        socket.abort();
                    }
                }
            }
            Err(_) => {
                socket.close();
            }
        }
    }
}

fn parse_request<'a, 'b>(
    buffer: &'a [u8],
    request_headers: &'b mut [Header<'a>],
) -> Result<(Request<'b, 'a>, &'a [u8]), httparse::Error> {
    let mut request = httparse::Request::new(request_headers);
    let status = request.parse(buffer)?;

    //Partial requests currently not supported
    if status.is_partial() {
        return Err(httparse::Error::Status);
    }

    let offset = status.unwrap();

    let mut body = &buffer[offset..];

    for i in 0..body.len() {
        //terminate on null
        if body[i] == 0 {
            body = &buffer[offset..offset + i];
            break;
        }
    }

    return Ok((request, body));
}

fn unsupported_request_handler<const SIZE: usize>(_request: Request) -> String<SIZE> {
    let mut response = String::<SIZE>::new();
    response
        .push_str("HTTP/1.1 501 Not Implemented\r\n\r\n")
        .unwrap();
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_request() {
        let request = "GET / HTTP/1.1\r\n\r\n".as_bytes();

        let mut request_headers = [EMPTY_HEADER; 2];
        let (request, body) = parse_request(request, &mut request_headers).unwrap();

        assert_eq!(request.method.unwrap(), "GET");
        assert_eq!(request.path.unwrap(), "/");

        let response: String<1024> = default_pages::index_get(request, body);

        assert_eq!(
            "HTTP/1.1 200 OK\r\n\r\n<!DOCTYPE html>
<html>
    <head>
        <title>DICE - Index</title>
    </head>
    <body>
        <h1>Welcome to DICE Index</h1>
        <p>hello</p>
    </body>
</html>",
            response
        );
    }

    #[test]
    fn http_server_test_page_get() {
        let request = "GET /test HTTP/1.1\r\n\r\n".as_bytes();

        let mut request_headers = [EMPTY_HEADER; 2];
        let (request, body) = parse_request(request, &mut request_headers).unwrap();

        assert_eq!(request.method.unwrap(), "GET");
        assert_eq!(request.path.unwrap(), "/test");

        let response: String<1024> = default_pages::test_page_get(request, body);

        assert_eq!(
            "HTTP/1.1 200 OK\r\n\r\n<!DOCTYPE html>
<html>
    <head>
        <title>DICE - test page</title>
    </head>
    <body>
        <h1>Welcome to DICE test page</h1>
    </body>
</html>",
            response
        );
    }

    #[test]
    fn http_server_test_404() {
        let request = "GET /cool_page HTTP/1.1\r\n\r\n".as_bytes();

        let mut request_headers = [EMPTY_HEADER; 2];
        let (request, body) = parse_request(request, &mut request_headers).unwrap();

        assert_eq!(request.method.unwrap(), "GET");
        assert_eq!(request.path.unwrap(), "/cool_page");

        let response: String<1024> = response::not_found_response();

        assert_eq!(
            "HTTP/1.1 404 Not Found\r\n\r\n<!DOCTYPE html>
<html>
    <head>
        <title>DICE - 404 not found</title>
    </head>
    <body>
        <h1>404 not found</h1>
    </body>
</html>",
            response
        );
    }
}
