mod binance;
mod coinbase;
mod gateio;
mod kucoin;
mod mexc;
mod okx;
mod poloniex;

use ic_cdk::export::candid::{decode_args, encode_args, Error as CandidError};
use serde::de::DeserializeOwned;

use crate::{utils, ONE_KIB};
use crate::{ExtractError, RATE_UNIT};
use crate::{DAI, USDC, USDT};

/// This macro generates the necessary boilerplate when adding an exchange to this module.
macro_rules! exchanges {
    ($($name:ident),*) => {
        /// Enum that contains all of the supported cryptocurrency exchanges.
        #[derive(PartialEq)]
        pub enum Exchange {
            $(
                #[allow(missing_docs)]
                $name($name),
            )*
        }

        $(
            #[derive(PartialEq)]
            pub struct $name;
        )*

        impl core::fmt::Display for Exchange {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $(Exchange::$name(_) => write!(f, stringify!($name))),*,
                }
            }
        }

        /// Contains all of the known exchanges that can be found in the
        /// [Exchange] enum.
        pub const EXCHANGES: &'static [Exchange] = &[
            $(Exchange::$name($name)),*
        ];


        /// Implements the core functionality of the generated `Exchange` enum.
        impl Exchange {

            /// Retrieves the position of the exchange in the EXCHANGES array.
            pub fn get_index(&self) -> usize {
                EXCHANGES.iter().position(|e| e == self).expect("should contain the exchange")
            }

            /// This method returns the formatted URL for the exchange.
            pub fn get_url(&self, base_asset: &str, quote_asset: &str, timestamp: u64) -> String {
                match self {
                    $(Exchange::$name(exchange) => exchange.get_url(base_asset, quote_asset, timestamp)),*,
                }
            }

            /// This method extracts the rate encoded in the given input.
            pub fn extract_rate(&self, bytes: &[u8]) -> Result<u64, ExtractError> {
                match self {
                    $(Exchange::$name(exchange) => exchange.extract_rate(bytes)),*,
                }
            }

            /// This method checks if the exchange supports IPv6.
            pub fn supports_ipv6(&self) -> bool {
                match self {
                    $(Exchange::$name(exchange) => exchange.supports_ipv6()),*,
                }
            }

            /// This method lists the supported stablecoin pairs of the exchange.
            pub fn supported_stablecoin_pairs(&self) -> &[(&str, &str)] {
                match self {
                    $(Exchange::$name(exchange) => exchange.supported_stablecoin_pairs()),*,
                }
            }

            /// Encodes the context in relation to the current exchange.
            pub fn encode_context(&self) -> Result<Vec<u8>, CandidError> {
                let index = self.get_index();
                encode_args((index,))
            }

            /// A general method to decode contexts from an `Exchange`.
            pub fn decode_context(bytes: &[u8]) -> Result<usize, CandidError> {
                decode_args::<(usize,)>(bytes).map(|decoded| decoded.0)
            }

            /// Encodes the response in the exchange transform method.
            pub fn encode_response(rate: u64) -> Result<Vec<u8>, CandidError> {
                encode_args((rate,))
            }

            /// Decodes the response from the exchange transform method.
            pub fn decode_response(bytes: &[u8]) -> Result<u64, CandidError> {
                decode_args::<(u64,)>(bytes).map(|decoded| decoded.0)
            }

            /// This method returns the exchange's max response bytes.
            pub fn max_response_bytes(&self) -> u64 {
                match self {
                    $(Exchange::$name(exchange) => exchange.max_response_bytes()),*,
                }
            }

            /// This method returns whether the exchange should be called. Availability
            /// is determined by whether or not the `ipv4-support` flag was used to compile the
            /// canister or the exchange supports IPv6 out-of-the-box.
            ///
            /// NOTE: This will be removed when IPv4 support is added to HTTP outcalls.
            pub fn is_available(&self) -> bool {
                utils::is_ipv4_support_available() || self.supports_ipv6()
            }
        }
    }

}

exchanges! { Binance, Coinbase, KuCoin, Okx, GateIo, Mexc, Poloniex }

/// Used to determine how to parse the extracted value returned from
/// [extract_rate]'s `extract_fn` argument.
enum ExtractedValue {
    Str(String),
    Float(f64),
}

/// This function provides a generic way to extract a rate out of the provided bytes.
fn extract_rate<R: DeserializeOwned>(
    bytes: &[u8],
    extract_fn: impl FnOnce(R) -> Option<ExtractedValue>,
) -> Result<u64, ExtractError> {
    let response = serde_json::from_slice::<R>(bytes)
        .map_err(|err| ExtractError::json_deserialize(bytes, err.to_string()))?;
    let extracted_value = extract_fn(response).ok_or_else(|| ExtractError::extract(bytes))?;

    let rate = match extracted_value {
        ExtractedValue::Str(value) => value
            .parse::<f64>()
            .map_err(|err| ExtractError::json_deserialize(bytes, err.to_string()))?,
        ExtractedValue::Float(value) => value,
    };

    Ok((rate * RATE_UNIT as f64) as u64)
}

/// The base URL may contain the following placeholders:
/// `BASE_ASSET`: This string must be replaced with the base asset string in the request.
const BASE_ASSET: &str = "BASE_ASSET";
/// `QUOTE_ASSET`: This string must be replaced with the quote asset string in the request.
const QUOTE_ASSET: &str = "QUOTE_ASSET";
/// `START_TIME`: This string must be replaced with the start time derived from the timestamp in the request.
const START_TIME: &str = "START_TIME";
/// `END_TIME`: This string must be replaced with the end time derived from the timestamp in the request.
const END_TIME: &str = "END_TIME";

/// This trait is use to provide the basic methods needed for an exchange.
trait IsExchange {
    /// The base URL template that is provided to [IsExchange::get_url].
    fn get_base_url(&self) -> &str;

    /// Provides the ability to format an asset code. Default implementation is
    /// to return the code as uppercase.
    fn format_asset(&self, asset: &str) -> String {
        asset.to_uppercase()
    }

    /// Provides the ability to format the start time. Default implementation is
    /// to simply return the provided timestamp as a string.
    fn format_start_time(&self, timestamp: u64) -> String {
        timestamp.to_string()
    }

    /// Provides the ability to format the end time. Default implementation is
    /// to simply return the provided timestamp as a string.
    fn format_end_time(&self, timestamp: u64) -> String {
        timestamp.to_string()
    }

    /// A default implementation to generate a URL based on the given parameters.
    /// The method takes the base URL for the exchange and replaces the following
    /// placeholders:
    /// * [BASE_ASSET]
    /// * [QUOTE_ASSET]
    /// * [START_TIME]
    /// * [END_TIME]
    fn get_url(&self, base_asset: &str, quote_asset: &str, timestamp: u64) -> String {
        let timestamp = (timestamp / 60) * 60;
        self.get_base_url()
            .replace(BASE_ASSET, &self.format_asset(base_asset))
            .replace(QUOTE_ASSET, &self.format_asset(quote_asset))
            .replace(START_TIME, &self.format_start_time(timestamp))
            .replace(END_TIME, &self.format_end_time(timestamp))
    }

    /// The implementation to extract the rate from the response's body.
    fn extract_rate(&self, bytes: &[u8]) -> Result<u64, ExtractError>;

    /// Indicates if the exchange supports IPv6.
    fn supports_ipv6(&self) -> bool {
        false
    }

    fn supported_stablecoin_pairs(&self) -> &[(&str, &str)] {
        &[(DAI, USDT), (USDC, USDT)]
    }

    fn max_response_bytes(&self) -> u64 {
        ONE_KIB
    }
}

#[cfg(test)]
mod test {
    use crate::utils::test::load_file;

    use super::*;

    /// The function tests the ability of an [Exchange] to encode the context to be sent
    /// to the exchange transform function.
    #[test]
    fn encode_context() {
        let exchange = Exchange::Coinbase(Coinbase);
        let bytes = exchange
            .encode_context()
            .expect("should encode Coinbase's index in EXCHANGES");
        let hex_string = hex::encode(bytes);
        assert_eq!(hex_string, "4449444c0001780100000000000000");
    }

    /// The function tests the ability of [Exchange] to encode a response body from the
    /// exchange transform function.
    #[test]
    fn encode_response() {
        let bytes = Exchange::encode_response(100).expect("should be able to encode value");
        let hex_string = hex::encode(bytes);
        assert_eq!(hex_string, "4449444c0001786400000000000000");
    }

    /// The function tests the ability of [Exchange] to decode a context in the exchange
    /// transform function.
    #[test]
    fn decode_context() {
        let hex_string = "4449444c0001780100000000000000";
        let bytes = hex::decode(hex_string).expect("should be able to decode");
        let result = Exchange::decode_context(&bytes);
        assert!(matches!(result, Ok(index) if index == 1));
    }

    /// The function tests the ability of [Exchange] to decode a response body from the
    /// exchange transform function.
    #[test]
    fn decode_response() {
        let hex_string = "4449444c0001786400000000000000";
        let bytes = hex::decode(hex_string).expect("should be able to decode");
        let result = Exchange::decode_response(&bytes);
        assert!(matches!(result, Ok(rate) if rate == 100));
    }

    #[test]
    #[cfg(not(feature = "ipv4-support"))]
    fn is_available() {
        let available_exchanges_count = EXCHANGES.iter().filter(|e| e.is_available()).count();
        assert_eq!(available_exchanges_count, 3);
    }

    #[test]
    #[cfg(feature = "ipv4-support")]
    fn is_available_ipv4() {
        let available_exchanges_count = EXCHANGES.iter().filter(|e| e.is_available()).count();
        assert_eq!(available_exchanges_count, 7);
    }
}
