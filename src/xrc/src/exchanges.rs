use jaq_core::Val;

use crate::candid::{Asset, AssetClass};
use crate::jq::{self, ExtractError};

/// This macro generates the necessary boilerplate when adding an exchange to this module.

macro_rules! exchanges {
    ($($name:ident),*) => {
        /// Enum that contains all of the supported cryptocurrency exchanges.
        pub enum Exchange {
            $(
                #[allow(missing_docs)]
                $name($name),
            )*
        }

        $(pub struct $name;)*

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

            /// This method routes the request to the correct exchange's [IsExchange::get_url] method.
            pub fn get_url(&self, base_asset: &str, quote_asset: &str, timestamp: u64) -> String {
                match self {
                    $(Exchange::$name(exchange) => exchange.get_url(base_asset, quote_asset, timestamp)),*,
                }
            }

            /// This method routes the response's body and the timestamp to the correct exchange's
            /// [IsExchange::extract_rate].
            pub fn extract_rate(&self, bytes: &[u8]) -> Result<u64, ExtractError> {
                match self {
                    $(Exchange::$name(exchange) => exchange.extract_rate(bytes)),*,
                }
            }

            /// This method invokes the exchange's [IsExchange::supports_ipv6] function.
            pub fn supports_ipv6(&self) -> bool {
                match self {
                    $(Exchange::$name(exchange) => exchange.supports_ipv6()),*,
                }
            }

            /// This method invokes the exchange's [IsExchange::supported_fiat_currencies] function.
            pub fn supported_fiat_currencies(&self) -> Vec<&str> {
                match self {
                    $(Exchange::$name(exchange) => exchange.supported_fiat_currencies()),*,
                }
            }

            /// This method invokes the exchange's [IsExchange::supported_usd_asset_type] function.
            pub fn supported_usd_asset_type(&self) -> Asset {
                match self {
                    $(Exchange::$name(exchange) => exchange.supported_usd_asset_type()),*,
                }
            }
        }
    }

}

exchanges! { Binance, Coinbase, KuCoin, Okx }

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
    /// The filter template that is provided to [IsExchange::extract_rate].
    fn get_filter(&self) -> &str;

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

    /// A default implementation to extract the rate from the response's body
    /// using the base filter and [jq::extract].
    fn extract_rate(&self, bytes: &[u8]) -> Result<u64, ExtractError> {
        let filter = self.get_filter();
        let value = jq::extract(bytes, filter)?;
        match value {
            Val::Num(rc) => match (*rc).as_f64() {
                Some(rate) => Ok((rate * 10_000.0) as u64),
                None => Err(ExtractError::InvalidNumericRate {
                    filter: filter.to_string(),
                    value: rc.to_string(),
                }),
            },
            _ => Err(ExtractError::RateNotFound {
                filter: filter.to_string(),
            }),
        }
    }

    /// Indicates if the exchange supports IPv6.
    fn supports_ipv6(&self) -> bool {
        false
    }

    /// Return the list of symbols of supported fiat currencies.
    fn supported_fiat_currencies(&self) -> Vec<&str> {
        vec![]
    }

    /// Return the exchange's supported USD asset type.
    fn supported_usd_asset_type(&self) -> Asset {
        Asset {
            symbol: "USDT".to_string(),
            class: AssetClass::Cryptocurrency,
        }
    }
}

/// Binance
impl IsExchange for Binance {
    fn get_filter(&self) -> &str {
        ".[0][1] | tonumber"
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
impl IsExchange for Coinbase {
    fn get_filter(&self) -> &str {
        ".[0][3]"
    }

    fn get_base_url(&self) -> &str {
        "https://api.pro.coinbase.com/products/BASE_ASSET-QUOTE_ASSET/candles?granularity=60&start=START_TIME&end=END_TIME"
    }

    fn supports_ipv6(&self) -> bool {
        true
    }

    fn supported_fiat_currencies(&self) -> Vec<&str> {
        vec!["USD", "EUR", "GBP"]
    }

    fn supported_usd_asset_type(&self) -> Asset {
        Asset {
            symbol: "USD".to_string(),
            class: AssetClass::FiatCurrency,
        }
    }
}

/// KuCoin
impl IsExchange for KuCoin {
    fn get_filter(&self) -> &str {
        ".data[0][1] | tonumber"
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
}

/// OKX
impl IsExchange for Okx {
    fn get_filter(&self) -> &str {
        ".data[0][1] | tonumber"
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

#[cfg(test)]
mod test {
    use super::*;

    /// The function test if the macro correctly generates the
    /// [core::fmt::Display] trait's implementation for [Exchange].
    #[test]
    fn exchange_to_string_returns_name() {
        let exchange = Exchange::Binance(Binance);
        assert_eq!(exchange.to_string(), "Binance");
        let exchange = Exchange::Coinbase(Coinbase);
        assert_eq!(exchange.to_string(), "Coinbase");
        let exchange = Exchange::KuCoin(KuCoin);
        assert_eq!(exchange.to_string(), "KuCoin");
        let exchange = Exchange::Okx(Okx);
        assert_eq!(exchange.to_string(), "Okx");
    }

    /// The function tests if the if the macro correctly generates derive copies by
    /// verifying that the exchanges return the correct query string.
    #[test]
    fn query_string_test() {
        // Note that the seconds are ignored, setting the considered timestamp to 1661523960.
        let timestamp = 1661524016;
        let binance = Binance;
        let query_string = binance.get_url("btc", "icp", timestamp);
        assert_eq!(query_string, "https://api.binance.com/api/v3/klines?symbol=BTCICP&interval=1m&startTime=1661523960000&endTime=1661523960000");

        let coinbase = Coinbase;
        let query_string = coinbase.get_url("btc", "icp", timestamp);
        assert_eq!(query_string, "https://api.pro.coinbase.com/products/BTC-ICP/candles?granularity=60&start=1661523960&end=1661523960");

        let kucoin = KuCoin;
        let query_string = kucoin.get_url("btc", "icp", timestamp);
        assert_eq!(query_string, "https://api.kucoin.com/api/v1/market/candles?symbol=BTC-ICP&type=1min&startAt=1661523960&endAt=1661523961");

        let okx = Okx;
        let query_string = okx.get_url("btc", "icp", timestamp);
        assert_eq!(query_string, "https://www.okx.com/api/v5/market/history-candles?instId=BTC-ICP&bar=1m&before=1661523959999&after=1661523960001");
    }

    /// The function test if the information about IPv6 support is correct.
    #[test]
    fn ipv6_support_test() {
        let binance = Binance;
        assert!(!binance.supports_ipv6());
        let coinbase = Coinbase;
        assert!(coinbase.supports_ipv6());
        let kucoin = KuCoin;
        assert!(kucoin.supports_ipv6());
        let okx = Okx;
        assert!(okx.supports_ipv6());
    }

    /// The function test if the information about fiat currency support is correct.
    #[test]
    fn fiat_currency_support_test() {
        let empty_vector: Vec<&str> = vec![];
        let binance = Binance;
        assert_eq!(binance.supported_fiat_currencies(), empty_vector);
        let coinbase = Coinbase;
        assert_eq!(
            coinbase.supported_fiat_currencies(),
            vec!["USD", "EUR", "GBP"]
        );
        let kucoin = KuCoin;
        assert_eq!(kucoin.supported_fiat_currencies(), empty_vector);
        let okx = Okx;
        assert_eq!(okx.supported_fiat_currencies(), empty_vector);
    }

    /// The function test if the information about IPv6 support is correct.
    #[test]
    fn supported_usd_asset_type() {
        let binance = Binance;
        assert_eq!(
            binance.supported_usd_asset_type(),
            Asset {
                symbol: "USDT".to_string(),
                class: AssetClass::Cryptocurrency
            }
        );
        let coinbase = Coinbase;
        assert_eq!(
            coinbase.supported_usd_asset_type(),
            Asset {
                symbol: "USD".to_string(),
                class: AssetClass::FiatCurrency
            }
        );
        let kucoin = KuCoin;
        assert_eq!(
            kucoin.supported_usd_asset_type(),
            Asset {
                symbol: "USDT".to_string(),
                class: AssetClass::Cryptocurrency
            }
        );
        let okx = Okx;
        assert_eq!(
            okx.supported_usd_asset_type(),
            Asset {
                symbol: "USDT".to_string(),
                class: AssetClass::Cryptocurrency
            }
        );
    }

    /// The function tests if the Binance struct returns the correct exchange rate.
    #[test]
    fn extract_rate_from_binance_test() {
        let binance = Binance;
        let query_response = r#"[[1637161920000,"41.96000000","42.07000000","41.96000000","42.06000000","771.33000000",1637161979999,"32396.87850000",63,"504.38000000","21177.00270000","0"]]"#
            .as_bytes();
        let extracted_rate = binance.extract_rate(query_response);
        assert!(matches!(extracted_rate, Ok(rate) if rate == 419_600));
    }

    /// The function tests if the Coinbase struct returns the correct exchange rate.
    #[test]
    fn extract_rate_from_coinbase_test() {
        let coinbase = Coinbase;
        let query_response = "[[1647734400,49.15,60.28,49.18,60.19,12.4941909]]".as_bytes();
        let extracted_rate = coinbase.extract_rate(query_response);
        assert!(matches!(extracted_rate, Ok(rate) if rate == 491_800));
    }

    /// The function tests if the Coinbase struct returns the correct exchange rate.
    #[test]
    fn extract_rate_from_kucoin_test() {
        let kucoin = KuCoin;
        let query_response = r#"{"code":"200000","data":[["1620296820","345.426","344.396","345.426", "344.096","280.47910557","96614.19641390067"]]}"#
            .as_bytes();
        let extracted_rate = kucoin.extract_rate(query_response);
        assert!(matches!(extracted_rate, Ok(rate) if rate == 3_454_260));
    }

    /// The function tests if the OKX struct returns the correct exchange rate.
    #[test]
    fn extract_rate_from_okx_test() {
        let okx = Okx;
        let query_response = r#"{"code":"0","msg":"","data":[["1637161920000","41.96","42.07","41.95","42.07","461.846542","19395.517323"]]}"#
            .as_bytes();
        let extracted_rate = okx.extract_rate(query_response);
        assert!(matches!(extracted_rate, Ok(rate) if rate == 419_600));
    }
}
