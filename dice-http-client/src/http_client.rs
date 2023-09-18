//Adding this because rust won't stop complaining about some variables being mutable, while in fact,
//they need to be in order for program to compile
#![allow(unused_assignments)]

use drogue_network::tcp::TcpStack;
use heapless::{String, Vec};
use httparse::{Response, EMPTY_HEADER};

#[derive(Debug)]
pub struct HttpHeader {
    pub name: String<64>,
    pub value: String<128>,
}

#[derive(Debug)]
pub struct HttpResponse<const MAX_RESPONSE_LENGTH: usize> {
    pub status: u16,
    pub headers: Vec<HttpHeader, 32>,
    pub content: String<MAX_RESPONSE_LENGTH>,
}

pub fn read_response<StackT: TcpStack, const MAX_RESPONSE_LENGTH: usize>(
    stack: &mut StackT,
    socket: &mut StackT::TcpSocket,
) -> Option<HttpResponse<MAX_RESPONSE_LENGTH>> {
    let mut buffer = [0; 2048];

    let mut full_response = String::<MAX_RESPONSE_LENGTH>::new();
    let mut headers = Vec::new();

    let mut offset: usize = 0;
    let mut status: u16 = 0;

    let mut content_bytes_read: usize = 0;
    let mut content_length: usize = 0;

    //Parse first read to get content length. We assume the entire response header will fit in first read

    let read_result = stack.read(socket, &mut buffer);

    if read_result.is_err() {
        return None;
    }

    let mut res_headers = [EMPTY_HEADER; 32];
    let mut response = Response::new(&mut res_headers);

    let parse_result = response.parse(&buffer);

    if parse_result.is_err() {
        return None;
    }

    offset = parse_result.unwrap().unwrap();
    status = response.code.unwrap();

    for header in res_headers.iter(){
        let new_header = HttpHeader {
            name: String::from(header.name),
            value: String::from(core::str::from_utf8(header.value).unwrap()),
        };

        headers.push(new_header).ok();

        if header.name == "Content-Length" {
            content_length = core::str::from_utf8(header.value)
                .unwrap()
                .parse::<usize>()
                .unwrap();
        }
    }
    

    let string = core::str::from_utf8(&buffer);

    if string.is_err() {
        return None;
    }

    let mut string = string.unwrap();

    let terminator_index = string.find("\0").unwrap_or(buffer.len());

    string = &string[0..terminator_index];
    content_bytes_read += string[offset..terminator_index].len();

    let _result = full_response.push_str(string);

    buffer.fill(0);

    //Read until we read content-length data
    //If server doesn't return content length header, read until socket is closed
    while content_bytes_read < content_length || content_length == 0 {

        let read_result = stack.read(socket, &mut buffer);

        match read_result {
            Ok(bytes_read) => {
                if bytes_read == 0 {
                    break;
                }

                let string = core::str::from_utf8(&buffer);

                if string.is_err() {
                    continue;
                }

                let mut string = string.unwrap();

                let terminator_index = string.find("\0").unwrap_or(buffer.len());

                string = &string[0..terminator_index];

                let _result = full_response.push_str(string);

                buffer.fill(0);
            }
            Err(_) => {
                break;
            }
        }   
    }

    let response = HttpResponse{
        status,
        headers,
        content: String::from(&full_response[offset..])
    };

    Some(response)
}
