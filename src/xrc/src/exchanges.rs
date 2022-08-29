use jaq_core::Val;

use crate::jq::{self, ExtractError};

/// This macro generates the necessary boilerplate when adding an exchange to this module.
/// For example,
///
/// ```
/// exchanges! { Coinbase }
/// ```
///
/// Generates the following:
///
/// ```
/// pub(crate) enum Exchange {
///     Coinbase(Coinbase)
/// }
///
/// pub(crate) Coinbase;
///
/// impl core::fmt::Display for Exchange {
///    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
///        match self {
///            Exchange::Coinbase(_) => write!(f, "Coinbase"),
///        }
///    }
/// }
///
/// pub(crate) const EXCHANGES: &'static [Exchange] = &[
///     Exchange::Coinbase(Coinbase),
/// ];
/// ```
macro_rules! exchanges {
    ($($name:ident),*) => {
        pub(crate) enum Exchange {
            $($name($name),)*
        }

        pub(crate) $(struct $name;)*

        impl core::fmt::Display for Exchange {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $(Exchange::$name(_) => write!(f, stringify!($name))),*,
                }
            }
        }

        pub(crate) const EXCHANGES: &'static [Exchange] = &[
            $(Exchange::$name($name)),*
        ];
    }
}

/// The interval size in seconds for which exchange rates are requested.
const REQUEST_TIME_INTERVAL_S: u64 = 60;

/// The base URL may contain the following placeholders:
/// `BASE_ASSET`: This string must be replaced with the base asset string in the request.
const BASE_ASSET: &str = "BASE_ASSET";
/// `QUOTE_ASSET`: This string must be replaced with the quote asset string in the request.
const QUOTE_ASSET: &str = "QUOTE_ASSET";
/// `START_TIME`: This string must be replaced with the start time derived from the timestamp in the request.
const START_TIME: &str = "START_TIME";
/// `END_TIME`: This string must be replaced with the end time derived from the timestamp in the request.
const END_TIME: &str = "END_TIME";
/// The base filter may contain the following placeholder:
/// `TIMESTAMP`: The timestamp of the requested exchange rate record.
const TIMESTAMP: &str = "TIMESTAMP";

exchanges! { Coinbase }

trait IsExchange {
    /// The base filter template for the
    fn get_base_filter(&self) -> &str;
    fn get_base_url(&self) -> &str;

    fn get_url(&self, base_asset: &str, quote_asset: &str, timestamp: u64) -> String {
        self.get_base_url()
            .replace(BASE_ASSET, &base_asset.to_uppercase())
            .replace(QUOTE_ASSET, &quote_asset.to_uppercase())
            .replace(
                START_TIME,
                &(timestamp - REQUEST_TIME_INTERVAL_S).to_string(),
            )
            .replace(END_TIME, &timestamp.to_string())
    }

    fn extract_rate(&self, bytes: &[u8], timestamp: u64) -> Result<u64, ExtractError> {
        let filter = self
            .get_base_filter()
            .replace(TIMESTAMP, &timestamp.to_string());
        let value = jq::extract(bytes, &filter)?;
        match value {
            Val::Num(rc) => match (*rc).as_f64() {
                Some(rate) => Ok((rate * 10_000.0) as u64),
                None => Err(ExtractError::Extraction {
                    filter: filter.clone(),
                    error: "Invalid numeric rate.".to_string(),
                }),
            },
            _ => Err(ExtractError::Extraction {
                filter,
                error: "Non-numeric rate.".to_string(),
            }),
        }
    }
}

impl Exchange {
    pub fn extract_rate(&self, bytes: &[u8], timestamp: u64) -> Result<u64, ExtractError> {
        match self {
            Exchange::Coinbase(coinbase) => coinbase.extract_rate(bytes, timestamp),
        }
    }

    pub fn get_url(&self, base_asset: &str, quote_asset: &str, timestamp: u64) -> String {
        match self {
            Exchange::Coinbase(coinbase) => coinbase.get_url(base_asset, quote_asset, timestamp),
        }
    }
}

impl IsExchange for Coinbase {
    fn get_base_filter(&self) -> &str {
        "map(select(.[0] == TIMESTAMP))[0][3]"
    }

    fn get_base_url(&self) -> &str {
        "https://api.pro.coinbase.com/products/BASE_ASSET-QUOTE_ASSET/candles?granularity=60&start=START_TIME&end=END_TIME"
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn exchange_to_string_returns_name() {
        let exchange = Exchange::Coinbase(Coinbase);
        assert_eq!(exchange.to_string(), "Coinbase");
    }

    /// The function tests if the Coinbase struct returns the correct query string.
    #[test]
    fn coinbase_query_string_test() {
        let coinbase = Coinbase;
        let query_string = coinbase.get_url("btc", "icp", 1661524016);
        assert_eq!(query_string, "https://api.pro.coinbase.com/products/BTC-ICP/candles?granularity=60&start=1661523956&end=1661524016");
    }

    /// The function tests if the Coinbase struct returns the correct exchange rate rate.
    #[test]
    fn coinbase_extract_rate_test() {
        let coinbase = Coinbase;
        let query_response = "[[1614596400,49.15,60.28,49.18,60.19,12.4941909],
            [1614596340,48.01,49.12,48.25,49.08,19.2031980]]"
            .as_bytes();
        let timestamp: u64 = 1614596340;
        let extracted_rate = coinbase.extract_rate(query_response, timestamp);
        assert!(matches!(extracted_rate, Ok(rate) if rate == 482_500));
    }
}
