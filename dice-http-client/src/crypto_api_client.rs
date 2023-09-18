use drogue_network::tcp::TcpStack;
use heapless::{FnvIndexMap,String, Vec};

#[derive(Debug)]
pub enum CryptoApiError{
    NoConnection,
    ParseError,
    RequestError,
    WriteError,
    ReadError,
}

pub trait CryptoApiClient<StackT: TcpStack, const MAX_CURRENCIES: usize>
{
    fn get_openday_price(
        network: &mut StackT,
        symbols: &Vec<String<16>, MAX_CURRENCIES>,
        currency: &str,
    ) -> Result<FnvIndexMap<String<16>, f32, MAX_CURRENCIES>, CryptoApiError>;

    fn get_current_prices(
        network: &mut StackT,
        symbols: &Vec<String<16>, MAX_CURRENCIES>,
        currency: &str,
    ) -> Result<FnvIndexMap<String<16>, f32, MAX_CURRENCIES>, CryptoApiError>;
}
