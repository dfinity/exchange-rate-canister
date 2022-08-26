use crate::http::CanisterHttpRequest;
use crate::jq::ExtractError;
use crate::{jq, types};
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
    /// The function returns the name of the exchange.
    fn get_name(&self) -> String;

    /// The function returns the full query string to request an exchange rate at a
    /// specific timestamp in UNIX epoch seconds.
    fn get_query_string(&self, base_asset: &str, quote_asset: &str, timestamp_s: u64) -> String;

    /// The function extracts the exchange rate from a returned response for a specific
    /// timestamp in UNIX epoch seconds if possible.
    fn extract_rate(&self, query_response: &[u8], timestamp_s: u64) -> ExtractRateResult;
}

/// Coinbase definitions
const COINBASE_BASE_QUERY: &str = "https://api.pro.coinbase.com/products/BASE_ASSET-QUOTE_ASSET/candles?granularity=60&start=START_TIME&end=END_TIME";

const COINBASE_FILTER: &str = "map(select(.[0] == TIMESTAMP))[0][3]";

/// The Coinbase exchange struct.
pub(crate) struct Coinbase;

impl core::fmt::Display for Coinbase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Coinbase")
    }
}

impl Exchange for Coinbase {
    fn get_name(&self) -> String {
        self.to_string()
    }

    fn get_query_string(&self, base_asset: &str, quote_asset: &str, timestamp: u64) -> String {
        COINBASE_BASE_QUERY
            .replace(BASE_ASSET, &base_asset.to_uppercase())
            .replace(QUOTE_ASSET, &quote_asset.to_uppercase())
            .replace(
                START_TIME,
                &(timestamp - REQUEST_TIME_INTERVAL_S).to_string(),
            )
            .replace(END_TIME, &timestamp.to_string())
    }

    fn extract_rate(&self, query_response: &[u8], timestamp: u64) -> ExtractRateResult {
        let filter = COINBASE_FILTER.replace(TIMESTAMP, &timestamp.to_string());
        let value = jq::extract(query_response, &filter)?;
        match value {
            Val::Num(rc) => match (*rc).as_f64() {
                Some(rate) => Ok((rate * 10_000.0) as u64),
                None => Err(ExtractError::Extraction {
                    filter: filter.clone(),
                    error: "Invalid numeric rate.".to_string(),
                }),
            },
            _ => Err(ExtractError::Extraction {
                filter: filter.clone(),
                error: "Non-numeric rate.".to_string(),
            }),
        }
    }
}

#[derive(Debug)]
pub enum CallExchangeError {
    Http {
        exchange: String,
        error: String,
    },
    Extract {
        exchange: String,
        error: ExtractError,
    },
}

impl core::fmt::Display for CallExchangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallExchangeError::Http { exchange, error } => {
                write!(f, "Failed to request from {exchange}: {error}")
            }
            CallExchangeError::Extract { exchange, error } => {
                write!(f, "Failed to extract rate from {exchange}: {error}")
            }
        }
    }
}

pub struct Exchanges {
    exchanges: Vec<Box<dyn Exchange>>,
}

impl Exchanges {
    pub fn new() -> Self {
        Self {
            exchanges: vec![Box::new(Coinbase)],
        }
    }

    pub async fn call(
        &self,
        args: &types::GetExchangeRateRequest,
    ) -> (Vec<u64>, Vec<CallExchangeError>) {
        let requests: Vec<_> = self
            .exchanges
            .iter()
            .map(|e| call_exchange(e, args))
            .collect();
        let mut rates = vec![];
        let mut errors = vec![];
        for request in requests {
            let result = request.await;
            match result {
                Ok(rate) => rates.push(rate),
                Err(error) => errors.push(error),
            };
        }
        (rates, errors)
    }
}

pub type CallExchangeResult = Result<u64, CallExchangeError>;

pub(crate) async fn call_exchange(
    exchange: &Box<dyn Exchange>,
    args: &types::GetExchangeRateRequest,
) -> CallExchangeResult {
    let timestamp_s = args.timestamp.unwrap_or_else(|| ic_cdk::api::time());
    let url = exchange.get_query_string(&args.base_asset, &args.quote_asset, timestamp_s);
    ic_cdk::println!("{}", url);
    let response = CanisterHttpRequest::new()
        .get(&url)
        .send()
        .await
        .map_err(|error| CallExchangeError::Http {
            exchange: exchange.get_name().to_string(),
            error,
        })?;
    exchange
        .extract_rate(&response.body, timestamp_s)
        .map_err(|error| CallExchangeError::Extract {
            exchange: exchange.get_name().to_string(),
            error,
        })
}

#[cfg(test)]
mod test {
    use super::*;

    /// The function tests if the Coinbase struct returns the correct query string.
    #[test]
    fn coinbase_query_string_test() {
        let coinbase = Coinbase;
        let query_string = coinbase.get_query_string("btc", "icp", 1661524016);
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
