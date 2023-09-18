//Some structs here use NONSNAKECASENAMES because they match the names of objects in parsed json
//We *could* change those names to match rust style guide and use serde macros to match rust name with json name
//But honestly, why bother?
#![allow(non_snake_case)]

use crate::{
    crypto_api_client::{CryptoApiClient, CryptoApiError},
    http_client::HttpResponse,
};
use drogue_network::{
    addr::{HostAddr, HostSocketAddr, IpAddr, Ipv4Addr},
    tcp::{Mode, TcpStack},
};
use heapless::{FnvIndexMap, LinearMap};
use heapless::{String, Vec};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct CryptoSelectedData {
    OPENDAY: f32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CryptoFullData {
    RAW: FnvIndexMap<String<16>, LinearMap<String<16>, CryptoSelectedData, 1>, 8>,
}

pub struct CryptoCompareApiClient;

impl<StackT: TcpStack, const MAX_CURRENCIES: usize> CryptoApiClient<StackT, MAX_CURRENCIES>
    for CryptoCompareApiClient
{
    fn get_openday_price(
        network: &mut StackT,
        symbols: &Vec<String<16>, MAX_CURRENCIES>,
        currency: &str,
    ) -> Result<FnvIndexMap<String<16>, f32, MAX_CURRENCIES>, CryptoApiError> {
        //Too much data to get in single request. Divide to chunks 6 cryptos each
        //Make single request for 6 cryptos
        let divided_symbols = symbols.chunks(6);
        let mut changes: FnvIndexMap<String<16>, f32, MAX_CURRENCIES> = FnvIndexMap::new();

        for chunk in divided_symbols {
            let request = construct_24_request(chunk, currency).unwrap();
            let mut i = 0;

            let result: Result<HttpResponse<16384>, CryptoApiError> = loop {
                if i > 3 {
                    break Err(CryptoApiError::NoConnection);
                }

                let socket = connect(network);

                if socket.is_err() {
                    break Err(CryptoApiError::NoConnection);
                }

                let mut socket = socket.unwrap();

                let result = try_send_24_request(network, &mut socket, request.as_bytes());

                if result.is_err() {
                    network.close(socket).unwrap();
                    i += 1;
                    continue;
                }

                let response = try_receive_24_request(network, &mut socket);

                if response.is_err() {
                    network.close(socket).unwrap();
                    i += 1;
                    continue;
                }

                break response;
            };

            if result.is_err() {
                return Err(CryptoApiError::NoConnection);
            }

            let response = result.unwrap();

            let content = trim_response(response.content.as_str())?;

            let parse_result: Result<(CryptoFullData, usize), _> =
                serde_json_core::from_str(content);

            match parse_result {
                Ok(res) => {
                    for (key, val) in res.0.RAW.iter() {
                        changes
                            .insert(key.clone(), val.values().next().unwrap().OPENDAY)
                            .unwrap();
                    }
                }
                Err(_) => {
                    return Err(CryptoApiError::ParseError);
                }
            }
        }

        return Ok(changes);
    }

    fn get_current_prices(
        network: &mut StackT,
        symbols: &Vec<String<16>, MAX_CURRENCIES>,
        currency: &str,
    ) -> Result<FnvIndexMap<String<16>, f32, MAX_CURRENCIES>, CryptoApiError> {
        let request = construct_price_request(symbols, currency)?;

        let mut socket = connect(network)?;

        let result = network.write(&mut socket, request.as_bytes());

        if result.is_err() {
            network.close(socket).unwrap();
            return Err(CryptoApiError::WriteError);
        }

        let response = crate::http_client::read_response::<_, 8192>(network, &mut socket);

        network.close(socket).unwrap();

        match response {
            Some(res) => {
                let content = trim_response(res.content.as_str())?;

                let result: Result<
                    (
                        FnvIndexMap<String<16>, LinearMap<String<16>, f32, 1>, MAX_CURRENCIES>,
                        usize,
                    ),
                    _,
                > = serde_json_core::from_str(content);

                match result {
                    Ok(res) => {
                        let mut result: FnvIndexMap<String<16>, f32, MAX_CURRENCIES> =
                            FnvIndexMap::new();
                        for (key, val) in res.0.iter() {
                            let price = val.values().next().unwrap();
                            result.insert(key.clone(), price.clone()).unwrap();
                        }

                        Ok(result)
                    }
                    Err(_) => Err(CryptoApiError::ParseError),
                }
            }
            None => Err(CryptoApiError::ReadError),
        }
    }
}

fn connect<StackT: TcpStack>(network: &mut StackT) -> Result<StackT::TcpSocket, CryptoApiError> {
    let ip = IpAddr::V4(Ipv4Addr::new(40, 115, 22, 134));
    let remote = HostSocketAddr::new(HostAddr::new(ip, None), 443);

    let socket = network.open(Mode::NonBlocking);

    if socket.is_err() {
        return Err(CryptoApiError::NoConnection);
    }

    let connection_result = network.connect(socket.unwrap(), remote);

    if connection_result.is_err() {
        return Err(CryptoApiError::NoConnection);
    }

    return Ok(connection_result.unwrap());
}

fn construct_24_request(
    symbols: &[String<16>],
    currency: &str,
) -> Result<String<1024>, CryptoApiError> {
    //Start building the get request
    let mut request: String<1024> = String::from("GET /data/pricemultifull?fsyms=");

    //Append symbols of selected cryptos
    for symbol in symbols {
        request
            .push_str(symbol.as_str())
            .map_err(|_| CryptoApiError::RequestError)?;
        request
            .push(',')
            .map_err(|_| CryptoApiError::RequestError)?;
    }

    //Add tsyms param - the currency to convert to
    request
        .push_str("&tsyms=")
        .map_err(|_| CryptoApiError::RequestError)?;
    request
        .push_str(currency)
        .map_err(|_| CryptoApiError::RequestError)?;

    //Finish by adding http version and Host header
    request
        .push_str(" HTTP/1.1\r\nHost: min-api.cryptocompare.com\r\n\r\n")
        .map_err(|_| CryptoApiError::RequestError)?;

    Ok(request)
}

fn try_send_24_request<StackT: TcpStack>(
    network: &mut StackT,
    socket: &mut StackT::TcpSocket,
    data: &[u8],
) -> Result<usize, CryptoApiError> {
    let result = network.write(socket, data);

    if result.is_err() {
        return Err(CryptoApiError::WriteError);
    } else {
        return Ok(result.unwrap());
    }
}

fn try_receive_24_request<StackT: TcpStack>(
    network: &mut StackT,
    socket: &mut StackT::TcpSocket,
) -> Result<HttpResponse<16384>, CryptoApiError> {
    let response = crate::http_client::read_response::<_, 16384>(network, socket);
    if response.is_none() {
        return Err(CryptoApiError::ReadError);
    } else {
        return Ok(response.unwrap());
    }
}

fn construct_price_request<const MAX_CURRENCIES: usize>(
    symbols: &Vec<String<16>, MAX_CURRENCIES>,
    currency: &str,
) -> Result<String<1024>, CryptoApiError> {
    //Start building the get request
    let mut request: String<1024> = String::from("GET /data/pricemulti?fsyms=");

    //Append symbols of selected cryptos
    for symbol in symbols {
        request
            .push_str(symbol.as_str())
            .map_err(|_| CryptoApiError::RequestError)?;
        request
            .push(',')
            .map_err(|_| CryptoApiError::RequestError)?;
    }

    //Add tsyms param - the currency to convert to
    request
        .push_str("&tsyms=")
        .map_err(|_| CryptoApiError::RequestError)?;
    request
        .push_str(currency)
        .map_err(|_| CryptoApiError::RequestError)?;

    //Finish by adding http version and Host header
    request
        .push_str(" HTTP/1.1\r\nHost: min-api.cryptocompare.com\r\n\r\n")
        .map_err(|_| CryptoApiError::RequestError)?;

    Ok(request)
}

//For some reason cryptocompare returns body with some numbers inserted before and after braces.
//The easy solution is to just trim the string to opening end closing braces of the JSON
fn trim_response(content: &str) -> Result<&str, CryptoApiError> {
    let trim_start = content.find('{');
    let trim_end = content.rfind('}');

    if !(trim_start.is_some() && trim_end.is_some()) {
        return Err(CryptoApiError::ParseError);
    }

    let content = &content[trim_start.unwrap()..trim_end.unwrap() + 1];

    Ok(content)
}
