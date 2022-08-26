use crate::jq;
use crate::jq::ExtractError;
use jaq_core::Val;

type ExtractRateResult = Result<u64, ExtractError>;

/// The interval size in seconds for which exchange rates are requested.
const REQUEST_TIME_INTERVAL_S: u64 = 60;

const BASE_ASSET: &str = "BASE_ASSET";
const QUOTE_ASSET: &str = "QUOTE_ASSET";
const START_TIME: &str = "START_TIME";
const END_TIME: &str = "END_TIME";
const TIMESTAMP: &str = "TIMESTAMP";

/// An exchange state consists of the exchange name, the base query,
/// and the filter to be applied to query results.
/// The base query may contain the following placeholders:
/// `BASE_ASSET`: This string must be replaced with the base asset string in the request.
/// `QUOTE_ASSET`: This string must be replaced with the quote asset string in the request.
/// `START_TIME`: This string must be replaced with the start time derived from the timestamp in the request.
/// `END_TIME`: This string must be replaced with the end time derived from the timestamp in the request.
/// The filter may contain the following placeholder:
/// `TIMESTAMP`: The timestamp of the requested exchange rate record.
struct ExchangeState {
    name: String,
    base_query: String,
    filter: String,
}

/// Every exchange struct must specify how an exchange rate at a specific point in time
/// must be queried. Moreover, it must define how to extract the rate from the response.
pub(crate) trait Exchange {
    /// The function returns an instance.
    fn new() -> Self;

    /// The function returns the name of the exchange.
    fn get_name(&self) -> &str;

    /// The function returns the full query string to request an exchange rate at a
    /// specific timestamp in UNIX epoch seconds.
    fn get_query_string(&self, base_asset: &str, quote_asset: &str, timestamp_s: u64) -> String;

    /// The function extracts the exchange rate from a returned response for a specific
    /// timestamp in UNIX epoch seconds if possible.
    fn extract_rate(&self, query_response: &[u8], timestamp_s: u64) -> ExtractRateResult;
}

/// The Coinbase exchange struct.
pub(crate) struct Coinbase {
    state: ExchangeState,
}

impl Exchange for Coinbase {
    fn new() -> Self {
        Coinbase {
            state: ExchangeState {
                name: "Coinbase".to_string(),
                base_query: "https://api.pro.coinbase.com/products/BASE_ASSET-QUOTE_ASSET/candles?granularity=60&start=START_TIME&end=END_TIME".to_string(),
                filter: "map(select(.[0] == TIMESTAMP))[0][3]".to_string(),
            }
        }
    }

    fn get_name(&self) -> &str {
        &self.state.name
    }

    fn get_query_string(&self, base_asset: &str, quote_asset: &str, timestamp: u64) -> String {
        self.state
            .base_query
            .replace(BASE_ASSET, &base_asset.to_uppercase())
            .replace(QUOTE_ASSET, &quote_asset.to_uppercase())
            .replace(
                START_TIME,
                &(timestamp - REQUEST_TIME_INTERVAL_S).to_string(),
            )
            .replace(END_TIME, &timestamp.to_string())
    }

    fn extract_rate(&self, query_response: &[u8], timestamp: u64) -> ExtractRateResult {
        let filter = self.state.filter.replace(TIMESTAMP, &timestamp.to_string());
        let value = jq::extract(query_response, &filter)?;
        match value {
            Val::Num(rc) => match (*rc).as_f64() {
                Some(rate) => Ok((rate * 10_000.0) as u64),
                None => Err(ExtractError::Extraction {
                    filter: self.state.filter.clone(),
                    error: "Invalid numeric rate.".to_string(),
                }),
            },
            _ => Err(ExtractError::Extraction {
                filter: self.state.filter.clone(),
                error: "Non-numeric rate.".to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    /// The function tests if the Coinbase struct returns the correct query string.
    #[test]
    fn coinbase_query_string_test() {
        let coinbase = Coinbase::new();
        let query_string = coinbase.get_query_string("btc", "icp", 1661524016);
        assert_eq!(query_string, "https://api.pro.coinbase.com/products/BTC-ICP/candles?granularity=60&start=1661523956&end=1661524016");
    }

    /// The function tests if the Coinbase struct returns the correct exchange rate rate.
    #[test]
    fn coinbase_extract_rate_test() {
        let coinbase = Coinbase::new();
        let query_response = "[[1614596400,49.15,60.28,49.18,60.19,12.4941909],
            [1614596340,48.01,49.12,48.25,49.08,19.2031980]]"
            .as_bytes();
        let timestamp: u64 = 1614596340;
        let extracted_rate = coinbase.extract_rate(query_response, timestamp);
        assert!(matches!(extracted_rate, Ok(rate) if rate == 482_500));
    }
}
