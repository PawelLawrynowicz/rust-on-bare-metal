//! Helper functions for generating Http responses.

use crate::default_pages::NOT_FOUND_HTML;
use heapless::String;

pub fn ok_response<const SIZE: usize>(content: &str) -> String<SIZE> {
    let mut response = String::<SIZE>::new();
    response.push_str("HTTP/1.1 200 OK\r\n\r\n").unwrap();
    response.push_str(content).unwrap();
    response
}

pub fn not_found_response<const SIZE: usize>() -> String<SIZE> {
    let mut response = String::<SIZE>::new();
    response.push_str("HTTP/1.1 404 Not Found\r\n\r\n").unwrap();
    response.push_str(NOT_FOUND_HTML).unwrap();
    response
}

pub fn not_found_no_body_response<const SIZE: usize>() -> String<SIZE> {
    let mut response = String::<SIZE>::new();
    response.push_str("HTTP/1.1 404 Not Found\r\n\r\n").unwrap();
    response
}

pub fn redirect_response<const SIZE: usize>(url: &str) -> String<SIZE> {
    let mut response = String::<SIZE>::new();
    response
        .push_str("HTTP/1.1 302 Found\r\nLocation: ")
        .unwrap();
    response.push_str(url).unwrap();
    response.push_str("\r\n\r\n").unwrap();
    response
}
