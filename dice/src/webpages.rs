use dice_http::response;

use heapless::{String, Vec};
use httparse::Request;

fn replace<const SIZE: usize>(source: &str, replaced: &str, replacement: &str) -> String<SIZE>{
    let pos = source.find(replaced).unwrap();

    let before_replaced = &source[0..pos];
    let after_replaced = &source[pos+replaced.len()..];

    let mut new_string = String::from(before_replaced);
    new_string.push_str(replacement).ok();
    new_string.push_str(after_replaced).ok();

    new_string
}

pub fn styles_get<const SIZE: usize>(_request: Request, _body: &[u8]) -> String<SIZE>{
    let styles = include_str!("webpages/styles.css");
    response::ok_response(styles)
}



pub fn index_get<const SIZE: usize>(symbols: &[&str]) -> String<SIZE> {

    let columns = symbols.chunks(16);

    let mut list_string = String::<SIZE>::new();

    for column in columns{
        list_string.push_str("<tr>\r\n").ok();
        for symbol in column{
            let mut entry_string = String::<128>::new();

            entry_string.push_str("<td>").ok();
            entry_string.push_str("<label>").ok();

            entry_string.push_str("<input class=\"check\" type=\"checkbox\" id=\">").ok();
            entry_string.push_str(symbol).ok();
            entry_string.push_str("\" name=\"").ok();
            entry_string.push_str(symbol).ok();
            entry_string.push_str("\" />").ok();

            entry_string.push_str(symbol).ok();

            entry_string.push_str("</label>").ok();
            entry_string.push_str("</td>\r\n").ok();

            list_string.push_str(entry_string.as_str()).ok();
        }
        list_string.push_str("\r\n</tr>").ok();
    }


    let page = include_str!("webpages/index.html");

    let page_string: String<SIZE> = replace(page, "{entries}", list_string.as_str());

    response::ok_response(page_string.as_str())
}

pub fn parse_post_body(body: &str) -> Vec<String<16>, 64>{
    let mut symbols = Vec::<String<16>, 64>::new();

    let splices = body.split('&');
    for splice in splices {
        if splice.len() == 0{
            break;
        }
        let end = splice.find('=').unwrap();
        let symbol = &splice[0 .. end];
        
        let _result = symbols.push(String::from(symbol));
    }

    return symbols;
}
