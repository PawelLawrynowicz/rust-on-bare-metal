//! Definitions of example route handlers.

use crate::response;
use core::str;
use heapless::String;

use httparse::Request;

pub const INDEX_HTML: &str = "<!DOCTYPE html>
<html>
    <head>
        <title>DICE - Index</title>
    </head>
    <body>
        <h1>Welcome to DICE Index</h1>
        <p>hello</p>
    </body>
</html>";

pub fn index_get<const SIZE: usize>(_request: Request, _body: &[u8]) -> String<SIZE> {
    response::ok_response(INDEX_HTML)
}

pub const TEST_PAGE_HTML: &str = "<!DOCTYPE html>
<html>
    <head>
        <title>DICE - test page</title>
    </head>
    <body>
        <h1>Welcome to DICE test page</h1>
    </body>
</html>";

pub fn test_page_get<const SIZE: usize>(_request: Request, _body: &[u8]) -> String<SIZE> {
    response::ok_response(TEST_PAGE_HTML)
}

pub const NOT_FOUND_HTML: &str = "<!DOCTYPE html>
<html>
    <head>
        <title>DICE - 404 not found</title>
    </head>
    <body>
        <h1>404 not found</h1>
    </body>
</html>";

pub fn test_page_post<const SIZE: usize>(_request: Request, body: &[u8]) -> String<SIZE> {
    let mut content = String::<256>::from(
        "<!DOCTYPE html>
    <html>
        <head>
            <title>DICE - Post Test</title>
        </head>
        <body>
            <h1>POST message:</h1>
    ",
    );

    let string = str::from_utf8(body).unwrap();

    content.push_str("<p>").ok();
    content.push_str(string).ok();
    content.push_str("</p>").ok();
    content.push_str("</body></html>").ok();

    response::ok_response(content.as_str())
}
