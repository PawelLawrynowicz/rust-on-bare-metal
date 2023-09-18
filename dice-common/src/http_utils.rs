#![allow(dead_code)]

use heapless::String;
use httparse::Status;

/// Generates a GET request
/// * `url` - a full url for resources you want to fetch
fn http_get(url: &str) -> String<256> {
    let mut request: String<256> = String::new();
    let (hostname, path) = url.split_at(url.chars().position(|x| x == '/').unwrap());

    request.push_str("GET ").unwrap();
    request.push_str(path).unwrap();
    request.push_str(" HTTP/1.1\r\nHost: ").unwrap();
    request.push_str(hostname).unwrap();
    request.push_str("\r\n\r\n").unwrap();

    request
}
/// Genertaes a POST request
/// * `url` - a full url for resources you want to fetch
/// * `content` - the body of the post request
fn http_post(url: &str, content: &str) -> String<256> {
    let mut request: String<256> = String::new();
    let (hostname, path) = url.split_at(url.chars().position(|x| x == '/').unwrap());

    request.push_str("POST ").unwrap();
    request.push_str(path).unwrap();
    request.push_str(" HTTP/1.1\r\nHost: ").unwrap();
    request.push_str(hostname).unwrap();
    request.push_str("\r\n\r\n").unwrap();
    request.push_str(content).unwrap();

    request
}

/// Prints the body of a POST request
/// * `post_req` - a post request in plaintext
fn get_body(post_req: &str) -> Result<&str, httparse::Error> {
    match validate_request(post_req) {
        Ok(_) => {
            let body = post_req.splitn(2, "\r\n\r\n").nth(1).unwrap();
            return Ok(body);
        }

        Err(e) => return Err(e),
    };
}

/// Generates a response to a request
/// * `req` - a request in plaintext
fn respond<'a>(req: &str) -> &'a str {
    let response = match validate_request(req) {
        Err(_) => "400 Bad Request",
        Ok(_) => "200 OK",
    };
    return response;
}

/// Validates if a given string is a valid HTTP request
/// * `input` - an input string
fn validate_request(input: &str) -> Result<Status<usize>, httparse::Error> {
    let mut headers = [httparse::EMPTY_HEADER; 16];
    let mut req = httparse::Request::new(&mut headers);

    // Unwraping partial request throws panic!, so it has to be a nested match statement
    let buf = input.as_bytes();
    match req.parse(buf) {
        Ok(status) => match status.is_complete() {
            true => return Ok(status),
            false => Err(httparse::Error::Status),
        },
        Err(e) => Err(e),
    }
}

#[cfg(test)]
#[test]
fn validate_request_realistic() {
    let req = "POST /api/TodoItems HTTP/1.1\r\nContent-Type: application/json\r\nUser-Agent: PostmanRuntime/7.26.10\r\nAccept: */*\r\nPostman-Token: 9cb5469f-5d87-48d6-89f6-208b4eec51fa\r\nHost: localhost:8000\r\nConnection: keep-alive\r\nContent-Length: 83\r\n\r\n{\r\n\"Id\":1,\r\n\"name\":\"aaa\",\r\n\"isComplete\":true,\r\n\"Secret\":\"secret\"\r\n}";
    let status_ok = match validate_request(req).unwrap() {
        Status::Complete(_) => true,
        _ => false,
    };
    assert!(status_ok);
}

#[test]
fn get_body_multiple_control_characters() {
    let req = "POST /item HTTP/1.0\r\nHost: pepe.frog\r\n\r\ntest body\r\n\r\n\r\n\r\n\r\n\r\n";
    assert_eq!(get_body(req).unwrap(), "test body\r\n\r\n\r\n\r\n\r\n\r\n");
}

#[test]
fn get_body_wrong_header() {
    let req = "POST /item HTTP/1.1\r\nContent-Length:5\r\nConnection: Keep-Alive\r\n\r\nHello";
    assert_eq!(get_body(req).unwrap(), "Hello");
}
#[test]
fn http_post_simple() {
    let url = "albicla.com/admin/password";
    let body = "show me ur password";
    assert_eq!(
        http_post(url, body),
        "POST /admin/password HTTP/1.1\r\nHost: albicla.com\r\n\r\nshow me ur password"
    );
}
#[test]
fn http_get_simple() {
    let url = "albicla.com/admin/password";
    assert_eq!(
        http_get(url),
        "GET /admin/password HTTP/1.1\r\nHost: albicla.com\r\n\r\n"
    );
}
#[test]
fn respond_bad_request() {
    let req = "POST /api/TodoItems HTTP/1.1\r\nERROR\r\nContent-Type: application/json\r\nUser-Agent: PostmanRuntime/7.26.10\r\nAccept: */*\r\nPostman-Token: 9cb5469f-5d87-48d6-89f6-208b4eec51fa\r\nHost: localhost:8000\r\nConnection: keep-alive\r\nContent-Length: 83\r\n\r\n{\r\n\"Id\":1,\r\n\"name\":\"aaa\",\r\n\"isComplete\":true,\r\n\"Secret\":\"secret\"\r\n}";
    assert_ne!(respond(req), "200 OK");
}
#[test]
fn respond_ok_request() {
    let req = "POST /api/TodoItems HTTP/1.1\r\nContent-Type: application/json\r\nUser-Agent: PostmanRuntime/7.26.10\r\nAccept: */*\r\nPostman-Token: 9cb5469f-5d87-48d6-89f6-208b4eec51fa\r\nHost: localhost:8000\r\nConnection: keep-alive\r\nContent-Length: 83\r\n\r\n{\r\n\"Id\":1,\r\n\"name\":\"aaa\",\r\n\"isComplete\":true,\r\n\"Secret\":\"secret\"\r\n}";
    assert_eq!(respond(req), "200 OK");
}
