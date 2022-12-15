#[cfg(test)]
mod tests;

use ic_cdk::export::{
    candid::{decode_args, encode_args, Error as CandidError},
    serde::Deserialize,
};
use serde::de::DeserializeOwned;

use crate::{
    candid::{Asset, AssetClass},
    utils, ONE_KIB,
};
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

            /// This method lists the USD assets supported by the exchange.
            pub fn supported_usd_asset_type(&self) -> Asset {
                match self {
                    $(Exchange::$name(exchange) => exchange.supported_usd_asset()),*,
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

exchanges! { Binance, Coinbase, KuCoin, Okx, GateIo, Mexc }

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

    /// Return the exchange's supported USD asset type.
    fn supported_usd_asset(&self) -> Asset {
        Asset {
            symbol: USDT.to_string(),
            class: AssetClass::Cryptocurrency,
        }
    }

    fn supported_stablecoin_pairs(&self) -> &[(&str, &str)] {
        &[(DAI, USDT), (USDC, USDT)]
    }

    fn max_response_bytes(&self) -> u64 {
        ONE_KIB
    }
}

/// Binance
type BinanceResponse = Vec<(
    u64,
    String,
    String,
    String,
    String,
    String,
    u64,
    String,
    u64,
    String,
    String,
    String,
)>;

impl IsExchange for Binance {
    fn extract_rate(&self, bytes: &[u8]) -> Result<u64, ExtractError> {
        extract_rate(bytes, |response: BinanceResponse| {
            response
                .get(0)
                .map(|kline| ExtractedValue::Str(kline.1.clone()))
        })
    }

    fn get_base_url(&self) -> &str {
        "https://api.binance.com/api/v3/klines?symbol=BASE_ASSETQUOTE_ASSET&interval=1m&startTime=START_TIME&endTime=END_TIME"
    }

    fn format_start_time(&self, timestamp: u64) -> String {
        // Convert seconds to milliseconds.
        timestamp.saturating_mul(1000).to_string()
    }

    fn format_end_time(&self, timestamp: u64) -> String {
        // Convert seconds to milliseconds.
        timestamp.saturating_mul(1000).to_string()
    }
}

/// Coinbase
type CoinbaseResponse = Vec<(u64, f64, f64, f64, f64, f64)>;

impl IsExchange for Coinbase {
    fn extract_rate(&self, bytes: &[u8]) -> Result<u64, ExtractError> {
        extract_rate(bytes, |response: CoinbaseResponse| {
            response.get(0).map(|kline| ExtractedValue::Float(kline.3))
        })
    }

    fn get_base_url(&self) -> &str {
        "https://api.pro.coinbase.com/products/BASE_ASSET-QUOTE_ASSET/candles?granularity=60&start=START_TIME&end=END_TIME"
    }

    fn supports_ipv6(&self) -> bool {
        true
    }

    fn supported_usd_asset(&self) -> Asset {
        Asset {
            symbol: "USD".to_string(),
            class: AssetClass::FiatCurrency,
        }
    }

    fn supported_stablecoin_pairs(&self) -> &[(&str, &str)] {
        &[(USDT, USDC)]
    }
}

/// KuCoin
#[derive(Deserialize)]
struct KuCoinResponse {
    data: Vec<(String, String, String, String, String, String, String)>,
}

impl IsExchange for KuCoin {
    fn extract_rate(&self, bytes: &[u8]) -> Result<u64, ExtractError> {
        extract_rate(bytes, |response: KuCoinResponse| {
            response
                .data
                .get(0)
                .map(|kline| ExtractedValue::Str(kline.1.clone()))
        })
    }

    fn get_base_url(&self) -> &str {
        "https://api.kucoin.com/api/v1/market/candles?symbol=BASE_ASSET-QUOTE_ASSET&type=1min&startAt=START_TIME&endAt=END_TIME"
    }

    fn format_end_time(&self, timestamp: u64) -> String {
        // In order to include the end time, a second must be added.
        timestamp.saturating_add(1).to_string()
    }

    fn supports_ipv6(&self) -> bool {
        true
    }

    fn supported_stablecoin_pairs(&self) -> &[(&str, &str)] {
        &[(USDC, USDT), (USDT, DAI)]
    }

    fn max_response_bytes(&self) -> u64 {
        2 * ONE_KIB
    }
}

/// OKX
/// https://www.okx.com/docs-v5/en/#rest-api-market-data-get-candlesticks

type OkxResponseDataEntry = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
);

#[derive(Deserialize)]
struct OkxResponse {
    data: Vec<OkxResponseDataEntry>,
}

impl IsExchange for Okx {
    fn extract_rate(&self, bytes: &[u8]) -> Result<u64, ExtractError> {
        extract_rate(bytes, |response: OkxResponse| {
            response
                .data
                .get(0)
                .map(|kline| ExtractedValue::Str(kline.1.clone()))
        })
    }

    fn get_base_url(&self) -> &str {
        // Counterintuitively, "after" specifies the end time, and "before" specifies the start time.
        "https://www.okx.com/api/v5/market/history-candles?instId=BASE_ASSET-QUOTE_ASSET&bar=1m&before=START_TIME&after=END_TIME"
    }

    fn format_start_time(&self, timestamp: u64) -> String {
        // Convert seconds to milliseconds and subtract 1 millisecond.
        timestamp.saturating_mul(1000).saturating_sub(1).to_string()
    }

    fn format_end_time(&self, timestamp: u64) -> String {
        // Convert seconds to milliseconds and add 1 millisecond.
        timestamp.saturating_mul(1000).saturating_add(1).to_string()
    }

    fn supports_ipv6(&self) -> bool {
        true
    }
}

/// Gate.io
type GateIoResponse = Vec<(String, String, String, String, String, String, String)>;

impl IsExchange for GateIo {
    fn extract_rate(&self, bytes: &[u8]) -> Result<u64, ExtractError> {
        extract_rate(bytes, |response: GateIoResponse| {
            response
                .get(0)
                .map(|kline| ExtractedValue::Str(kline.3.clone()))
        })
    }

    fn get_base_url(&self) -> &str {
        "https://api.gateio.ws/api/v4/spot/candlesticks?currency_pair=BASE_ASSET_QUOTE_ASSET&interval=1m&from=START_TIME&to=END_TIME"
    }

    fn supported_stablecoin_pairs(&self) -> &[(&str, &str)] {
        &[(DAI, USDT)]
    }
}

/// MEXC
#[derive(Deserialize)]
struct MexcResponse {
    data: Vec<(u64, String, String, String, String, String, String)>,
}

impl IsExchange for Mexc {
    fn extract_rate(&self, bytes: &[u8]) -> Result<u64, ExtractError> {
        extract_rate(bytes, |response: MexcResponse| {
            response
                .data
                .get(0)
                .map(|kline| ExtractedValue::Str(kline.1.clone()))
        })
    }

    fn get_base_url(&self) -> &str {
        "https://www.mexc.com/open/api/v2/market/kline?symbol=BASE_ASSET_QUOTE_ASSET&interval=1m&start_time=START_TIME&limit=1"
    }
}
