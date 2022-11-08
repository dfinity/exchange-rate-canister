#![warn(missing_docs)]

//! The XRC provides a powerful exchange rate API, which can be leveraged by
//! other applications, e.g., in the DeFi space.
// TODO: expand on this documentation

mod api;
mod cache;
/// This module provides the candid types to be used over the wire.
pub mod candid;
mod exchanges;
mod forex;
mod http;
mod stablecoin;

mod environment;
/// This module provides the ability to use `jq` filters on the returned
/// response bodies.
mod jq;
mod periodic;
mod rate_limiting;
/// This module provides types for responding to HTTP requests for metrics.
pub mod types;
mod utils;

use ::candid::{CandidType, Deserialize};
use ic_cdk::{
    api::management_canister::http_request::{HttpResponse, TransformArgs},
    export::candid::Principal,
};
use serde_bytes::ByteBuf;

use crate::{
    candid::{Asset, ExchangeRate, ExchangeRateMetadata},
    forex::ForexRateStore,
};
use cache::ExchangeRateCache;
use forex::{Forex, ForexContextArgs, ForexRateMap, FOREX_SOURCES};
use http::CanisterHttpRequest;
use std::cell::{Cell, RefCell};

pub use api::get_exchange_rate;
pub use api::usdt_asset;
pub use exchanges::{Exchange, EXCHANGES};
use utils::{median, standard_deviation_permyriad};

const LOG_PREFIX: &str = "[xrc]";

/// The number of cycles needed to use the `xrc` canister.
pub const XRC_REQUEST_CYCLES_COST: u64 = 5_000_000_000;

/// Id of the cycles minting canister on the IC (rkp4c-7iaaa-aaaaa-aaaca-cai).
const CYCLES_MINTING_CANISTER_ID: Principal =
    Principal::from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x01, 0x01]);

/// The currency symbol for the US dollar.
const USD: &str = "USD";

/// The symbol for the USDT stablecoin.
const USDT: &str = "USDT";

/// The symbol for the Dai stablecoin.
const DAI: &str = "DAI";

/// The symbol for the USDC stablecoin.
const USDC: &str = "USDC";

/// By default, cache entries are valid for 60 seconds.
const CACHE_RETENTION_PERIOD_SEC: u64 = 60;

/// Stablecoins are cached longer as they typically fluctuate less.
const STABLECOIN_CACHE_RETENTION_PERIOD_SEC: u64 = 600;

/// The maximum number of concurrent requests. Experiments show that 50 RPS can be handled.
/// Since a request triggers approximately 10 HTTP outcalls, 5 concurrent requests are permissible.
const MAX_NUM_CONCURRENT_REQUESTS: u64 = 5;

/// The soft max size of the cache.
/// Since each request takes around 3 seconds, there can be [MAX_NUM_CONCURRENT_REQUESTS] times
/// [CACHE_EXPIRATION_TIME_SEC] divided by 3 records collected in the cache.
const SOFT_MAX_CACHE_SIZE: usize =
    (MAX_NUM_CONCURRENT_REQUESTS * CACHE_RETENTION_PERIOD_SEC / 3) as usize;

/// The hard max size of the cache, which is simply twice the soft max size of the cache.
const HARD_MAX_CACHE_SIZE: usize = SOFT_MAX_CACHE_SIZE * 2;

thread_local! {
    // The exchange rate cache.
    static EXCHANGE_RATE_CACHE: RefCell<ExchangeRateCache> = RefCell::new(
        ExchangeRateCache::new(USDT.to_string(), SOFT_MAX_CACHE_SIZE, HARD_MAX_CACHE_SIZE));

    static FOREX_RATE_STORE: RefCell<ForexRateStore> = RefCell::new(ForexRateStore::new());

    /// The counter used to determine if a request should be rate limited or not.
    static RATE_LIMITING_REQUEST_COUNTER: Cell<usize> = Cell::new(0);

    static GET_EXCHANGE_RATE_REQUEST_COUNTER: Cell<usize> = Cell::new(0);
    static GET_EXCHANGE_RATE_REQUEST_FROM_CMC_COUNTER: Cell<usize> = Cell::new(0);
    static CYCLE_RELATED_ERRORS_COUNTER: Cell<usize> = Cell::new(0);
    static ERRORS_RETURNED_COUNTER: Cell<usize> = Cell::new(0);
    static ERRORS_RETURNED_TO_CMC_COUNTER: Cell<usize> = Cell::new(0);

}

/// Used to retrieve or increment the various metric counters in the state.
enum MetricCounter {
    /// Maps to the [GET_EXCHANGE_RATE_REQUEST_COUNTER].
    GetExchangeRateRequest,
    /// Maps to the [GET_EXCHANGE_RATE_REQUEST_FROM_CMC_COUNTER].
    GetExchangeRateRequestFromCmc,
    /// Maps to the [CYCLE_RELATED_ERRORS_COUNTER].
    CycleRelatedErrors,
    /// Maps to the [ERRORS_RETURNED_COUNTER].
    ErrorsReturned,
    /// Maps to the [ERRORS_RETURNED_TO_CMC_COUNTER].
    ErrorsReturnedToCmcCounter,
}

impl MetricCounter {
    fn get(&self) -> usize {
        match self {
            MetricCounter::GetExchangeRateRequest => {
                GET_EXCHANGE_RATE_REQUEST_COUNTER.with(|c| c.get())
            }
            MetricCounter::GetExchangeRateRequestFromCmc => {
                GET_EXCHANGE_RATE_REQUEST_FROM_CMC_COUNTER.with(|c| c.get())
            }
            MetricCounter::CycleRelatedErrors => CYCLE_RELATED_ERRORS_COUNTER.with(|c| c.get()),
            MetricCounter::ErrorsReturned => ERRORS_RETURNED_COUNTER.with(|c| c.get()),
            MetricCounter::ErrorsReturnedToCmcCounter => {
                ERRORS_RETURNED_TO_CMC_COUNTER.with(|c| c.get())
            }
        }
    }

    fn increment(&self) {
        match self {
            MetricCounter::GetExchangeRateRequest => {
                GET_EXCHANGE_RATE_REQUEST_COUNTER.with(|c| c.set(c.get().saturating_add(1)));
            }
            MetricCounter::GetExchangeRateRequestFromCmc => {
                GET_EXCHANGE_RATE_REQUEST_FROM_CMC_COUNTER
                    .with(|c| c.set(c.get().saturating_add(1)));
            }
            MetricCounter::CycleRelatedErrors => {
                CYCLE_RELATED_ERRORS_COUNTER.with(|c| c.set(c.get().saturating_add(1)));
            }
            MetricCounter::ErrorsReturned => {
                ERRORS_RETURNED_COUNTER.with(|c| c.set(c.get().saturating_add(1)));
            }
            MetricCounter::ErrorsReturnedToCmcCounter => {
                ERRORS_RETURNED_TO_CMC_COUNTER.with(|c| c.set(c.get().saturating_add(1)));
            }
        }
    }
}

fn with_cache<R>(f: impl FnOnce(&ExchangeRateCache) -> R) -> R {
    EXCHANGE_RATE_CACHE.with(|cache| f(&cache.borrow()))
}

fn with_cache_mut<R>(f: impl FnOnce(&mut ExchangeRateCache) -> R) -> R {
    EXCHANGE_RATE_CACHE.with(|cache| f(&mut cache.borrow_mut()))
}

/// A helper method to read the from the forex rate store.
fn with_forex_rate_store<R>(f: impl FnOnce(&ForexRateStore) -> R) -> R {
    FOREX_RATE_STORE.with(|cell| f(&cell.borrow()))
}

/// A helper method to mutate the forex rate store.
#[allow(dead_code)]
fn with_forex_rate_store_mut<R>(f: impl FnOnce(&mut ForexRateStore) -> R) -> R {
    FOREX_RATE_STORE.with(|cell| f(&mut cell.borrow_mut()))
}

/// The received rates for a particular exchange rate request are stored in this struct.
#[derive(CandidType, Deserialize, Clone, Debug, PartialEq)]
pub struct QueriedExchangeRate {
    /// The base asset.
    pub base_asset: Asset,
    /// The quote asset.
    pub quote_asset: Asset,
    /// The timestamp associated with the returned rate.
    pub timestamp: u64,
    /// The received rates in permyriad.
    pub rates: Vec<u64>,
    /// The number of queried exchanges for the base asset.
    pub base_asset_num_queried_sources: usize,
    /// The number of rates successfully received from the queried sources for the quote asset.
    pub base_asset_num_received_rates: usize,
    /// The number of queried exchanges for the base asset.
    pub quote_asset_num_queried_sources: usize,
    /// The number of rates successfully received from the queried sources for the quote asset.
    pub quote_asset_num_received_rates: usize,
}

impl Default for QueriedExchangeRate {
    fn default() -> Self {
        Self {
            base_asset: usdt_asset(),
            quote_asset: usdt_asset(),
            timestamp: Default::default(),
            rates: Default::default(),
            base_asset_num_queried_sources: Default::default(),
            base_asset_num_received_rates: Default::default(),
            quote_asset_num_queried_sources: Default::default(),
            quote_asset_num_received_rates: Default::default(),
        }
    }
}

impl std::ops::Mul for QueriedExchangeRate {
    type Output = Self;

    /// The function multiplies two [QueriedExchangeRate] structs.
    /// This is a meaningful operation if the quote asset of the first struct is
    /// identical to the base asset of the second struct.
    #[allow(clippy::suspicious_arithmetic_impl)]
    fn mul(self, other_rate: Self) -> Self {
        let mut rates = vec![];
        for own_value in self.rates {
            for other_value in other_rate.rates.iter() {
                rates.push(
                    own_value
                        .saturating_mul(*other_value)
                        .saturating_div(10_000),
                );
            }
        }
        Self {
            base_asset: self.base_asset,
            quote_asset: other_rate.quote_asset,
            timestamp: self.timestamp,
            rates,
            base_asset_num_queried_sources: self.base_asset_num_queried_sources,
            base_asset_num_received_rates: self.base_asset_num_received_rates,
            quote_asset_num_queried_sources: other_rate.quote_asset_num_queried_sources,
            quote_asset_num_received_rates: other_rate.quote_asset_num_received_rates,
        }
    }
}

impl std::ops::Div for QueriedExchangeRate {
    type Output = Self;

    /// The function divides two [QueriedExchangeRate] structs.
    /// This is a meaningful operation if the quote asset of the first struct is
    /// identical to the base asset of the second struct.
    #[allow(clippy::suspicious_arithmetic_impl)]
    fn div(self, other_rate: Self) -> Self {
        self * other_rate.inverted()
    }
}

impl From<QueriedExchangeRate> for ExchangeRate {
    fn from(rate: QueriedExchangeRate) -> Self {
        ExchangeRate {
            base_asset: rate.base_asset,
            quote_asset: rate.quote_asset,
            timestamp: rate.timestamp,
            rate_permyriad: median(&rate.rates),
            metadata: ExchangeRateMetadata {
                base_asset_num_queried_sources: rate.base_asset_num_queried_sources,
                base_asset_num_received_rates: rate.base_asset_num_received_rates,
                quote_asset_num_queried_sources: rate.quote_asset_num_queried_sources,
                quote_asset_num_received_rates: rate.quote_asset_num_received_rates,
                standard_deviation_permyriad: standard_deviation_permyriad(&rate.rates),
            },
        }
    }
}

impl QueriedExchangeRate {
    /// The function creates a [QueriedExchangeRate] instance based on a lookup for the given
    /// base-quote asset pair.
    pub(crate) fn new(
        base_asset: Asset,
        quote_asset: Asset,
        timestamp: u64,
        rates: &[u64],
        num_queried_sources: usize,
        num_received_rates: usize,
    ) -> QueriedExchangeRate {
        Self {
            base_asset,
            quote_asset,
            timestamp,
            rates: rates.to_vec(),
            base_asset_num_queried_sources: num_queried_sources,
            base_asset_num_received_rates: num_received_rates,
            quote_asset_num_queried_sources: 0,
            quote_asset_num_received_rates: 0,
        }
    }

    /// The function returns the exchange rate with base asset and quote asset inverted.
    pub(crate) fn inverted(&self) -> Self {
        let inverted_rates: Vec<_> = self
            .rates
            .iter()
            .map(|rate| utils::invert_rate(*rate))
            .collect();
        Self {
            base_asset: self.quote_asset.clone(),
            quote_asset: self.base_asset.clone(),
            timestamp: self.timestamp,
            rates: inverted_rates,
            base_asset_num_queried_sources: self.quote_asset_num_queried_sources,
            base_asset_num_received_rates: self.quote_asset_num_received_rates,
            quote_asset_num_queried_sources: self.base_asset_num_queried_sources,
            quote_asset_num_received_rates: self.base_asset_num_received_rates,
        }
    }
}

/// The arguments for the [call_exchanges] function.
#[derive(Clone)]
pub struct CallExchangeArgs {
    /// The timestamp provided by the user or the time from the IC.
    pub timestamp: u64,
    /// The asset to be used as the starting asset. For example, using
    /// ICP/USD, USD would be the quote asset.
    pub quote_asset: Asset,
    /// The asset to be used as the resulting asset. For example, using
    /// ICP/USD, ICP would be the base asset.
    pub base_asset: Asset,
}

/// The possible errors that can occur when calling an exchange.
#[derive(Clone, Debug)]
pub enum CallExchangeError {
    /// Error that occurs when making a request to the management canister's `http_request` endpoint.
    Http {
        /// The exchange that is associated with the error.
        exchange: String,
        /// The error that is returned from the management canister.
        error: String,
    },
    /// Error that occurs when extracting the rate from the response.
    Extract {
        /// The exchange that is associated with the error.
        exchange: String,
        /// The error that occurred while extracting the rate.
        error: ExtractError,
    },
    /// Error used when there is a failure encoding or decoding candid.
    Candid {
        /// The exchange that is associated with the error.
        exchange: String,
        /// The error returned from the candid encode/decode.
        error: String,
    },
    /// Error used when no rates have been found at all for an asset.
    NoRatesFound,
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
            CallExchangeError::Candid { exchange, error } => {
                write!(f, "Failed to encode/decode {exchange}: {error}")
            }
            CallExchangeError::NoRatesFound => {
                write!(f, "Failed to retrieve rates for asset")
            }
        }
    }
}

impl From<candid::GetExchangeRateRequest> for CallExchangeArgs {
    fn from(request: candid::GetExchangeRateRequest) -> Self {
        Self {
            timestamp: request.timestamp.unwrap_or_else(utils::time_secs),
            quote_asset: request.quote_asset,
            base_asset: request.base_asset,
        }
    }
}

async fn call_exchange(
    exchange: &Exchange,
    args: CallExchangeArgs,
) -> Result<u64, CallExchangeError> {
    let url = exchange.get_url(
        &args.base_asset.symbol,
        &args.quote_asset.symbol,
        args.timestamp,
    );
    let context = exchange
        .encode_context()
        .map_err(|error| CallExchangeError::Candid {
            exchange: exchange.to_string(),
            error: format!("Failure while encoding context: {}", error),
        })?;
    let response = CanisterHttpRequest::new()
        .get(&url)
        .transform_context("transform_exchange_http_response", context)
        .send()
        .await
        .map_err(|error| CallExchangeError::Http {
            exchange: exchange.to_string(),
            error,
        })?;

    Exchange::decode_response(&response.body).map_err(|error| CallExchangeError::Candid {
        exchange: exchange.to_string(),
        error: format!("Failure while decoding response: {}", error),
    })
}

/// This is used to collect all of the arguments needed for possibly sending a forex request.
#[allow(dead_code)]
struct CallForexArgs {
    timestamp: u64,
}

/// The possible errors that can occur when calling an exchange.
#[derive(Debug, Clone)]
enum CallForexError {
    /// Error that occurs when making a request to the management canister's `http_request` endpoint.
    Http {
        /// The forex that is associated with the error.
        forex: String,
        /// The error that is returned from the management canister.
        error: String,
    },
    /// Error used when there is a failure encoding or decoding candid.
    Candid {
        /// The forex that is associated with the error.
        forex: String,
        /// The error returned from the candid encode/decode.
        error: String,
    },
}

impl core::fmt::Display for CallForexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallForexError::Http { forex, error } => {
                write!(f, "Failed to request from {forex}: {error}")
            }
            CallForexError::Candid { forex, error } => {
                write!(f, "Failed to encode/decode {forex}: {error}")
            }
        }
    }
}

/// Function used to call a single forex with
#[allow(dead_code)]
async fn call_forex(
    forex: &Forex,
    args: &ForexContextArgs,
) -> Result<ForexRateMap, CallForexError> {
    let url = forex.get_url(args.timestamp);
    let context = forex
        .encode_context(args)
        .map_err(|error| CallForexError::Candid {
            forex: forex.to_string(),
            error: error.to_string(),
        })?;

    let response = CanisterHttpRequest::new()
        .get(&url)
        .transform_context("transform_forex_http_response", context)
        .send()
        .await
        .map_err(|error| CallForexError::Http {
            forex: forex.to_string(),
            error,
        })?;

    Forex::decode_response(&response.body).map_err(|error| CallForexError::Candid {
        forex: forex.to_string(),
        error: error.to_string(),
    })
}

/// Serializes the state and stores it in stable memory.
pub fn pre_upgrade() {
    with_forex_rate_store(|store| ic_cdk::storage::stable_save((store,)))
        .expect("Saving state must succeed.")
}

/// Deserializes the state from stable memory and sets the canister state.
pub fn post_upgrade() {
    let store = ic_cdk::storage::stable_restore::<(ForexRateStore,)>()
        .expect("Failed to read from stable memory.")
        .0;
    FOREX_RATE_STORE.with(|cell| {
        *cell.borrow_mut() = store;
    });
}

/// Called by the canister's heartbeat so periodic tasks can be executed.
pub fn heartbeat() {
    let timestamp = utils::time_secs();
    let future = periodic::run_tasks(timestamp);
    ic_cdk::spawn(future);
}

/// This function sanitizes the [HttpResponse] as requests must be idempotent.
/// Currently, this function strips out the response headers as that is the most
/// likely culprit to cause issues. Additionally, it extracts the rate from the response
/// and returns that in the body.
///
/// [Interface Spec - IC method `http_request`](https://internetcomputer.org/docs/current/references/ic-interface-spec/#ic-http_request)
pub fn transform_exchange_http_response(args: TransformArgs) -> HttpResponse {
    let mut sanitized = args.response;
    let index = Exchange::decode_context(&args.context).expect("Failed to decode context");

    // It should be ok to trap here as this does not modify state.
    let exchange = EXCHANGES
        .get(index)
        .expect("Provided index does not exist in exchanges.");

    let rate = exchange
        .extract_rate(&sanitized.body)
        .expect("Failed to extract rate from the body.");

    sanitized.body = Exchange::encode_response(rate).expect("Failed to encode rate");

    // Strip out the headers as these will commonly cause an error to occur.
    sanitized.headers = vec![];
    sanitized
}

/// This function sanitizes the [HttpResponse] as requests must be idempotent.
/// Currently, this function strips out the response headers as that is the most
/// likely culprit to cause issues. Additionally, it extracts the rate from the response
/// and returns that in the body.
///
/// [Interface Spec - IC method `http_request`](https://internetcomputer.org/docs/current/references/ic-interface-spec/#ic-http_request)
pub fn transform_forex_http_response(args: TransformArgs) -> HttpResponse {
    let mut sanitized = args.response;
    let context = Forex::decode_context(&args.context).expect("Failed to decode the context");
    let forex = FOREX_SOURCES
        .get(context.id)
        .expect("Invalid forex source ID provided in context");
    sanitized.body = forex
        .transform_http_response_body(&sanitized.body, &context.payload)
        .map_err(|err| format!("{}: {}", forex, err))
        .expect("Failed to extract rates");

    // Strip out the headers as these will commonly cause an error to occur.
    sanitized.headers = vec![];
    sanitized
}

/// This adds the ability to handle HTTP requests to the canister.
/// Used to expose metrics to prometheus.
pub fn http_request(req: types::HttpRequest) -> types::HttpResponse {
    let parts: Vec<&str> = req.url.split('?').collect();
    match parts[0] {
        "/metrics" => api::get_metrics(),
        _ => types::HttpResponse {
            status_code: 404,
            headers: vec![],
            body: ByteBuf::from(String::from("Not found.")),
        },
    }
}

/// Represents the errors when attempting to extract a value from JSON or XML.
#[derive(Clone, Debug)]
pub enum ExtractError {
    /// The provided input is not valid JSON.
    JsonDeserialize(String),
    /// The provided input is not valid XML.
    XmlDeserialize(String),
    /// The filter provided to extract cannot be used to create a `jq`-like filter.
    MalformedFilterExpression {
        /// The filter that was used when the error occurred.
        filter: String,
        /// The set of errors that were found when the filter was compiled.
        errors: Vec<String>,
    },
    /// The filter failed to extract from the JSON as the filter selects a value improperly.
    Extraction {
        /// The filter that was used when the error occurred.
        filter: String,
        /// The error from the filter that `jaq` triggered.
        error: String,
    },
    /// The filter found a rate, but it could not be converted to a valid form.
    InvalidNumericRate {
        /// The filter that was used when the error occurred.
        filter: String,
        /// The value that was extracted by the filter.
        value: String,
    },
    /// The filter executed but could not find a rate.
    RateNotFound {
        /// The filter that was used when the error occurred.
        filter: String,
    },
}

impl core::fmt::Display for ExtractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtractError::MalformedFilterExpression { filter, errors } => {
                let joined_errors = errors.join("\n");
                write!(f, "Parsing filter ({filter}) failed: {joined_errors}")
            }
            ExtractError::Extraction { filter, error } => {
                write!(
                    f,
                    "Extracting values with filter ({filter}) failed: {error}"
                )
            }
            ExtractError::JsonDeserialize(error) => {
                write!(f, "Failed to deserialize JSON: {error}")
            }
            ExtractError::XmlDeserialize(error) => {
                write!(f, "Failed to deserialize XML: {error}")
            }
            ExtractError::InvalidNumericRate { filter, value } => {
                write!(
                    f,
                    "Invalid numeric rate found with filter ({filter}): {value}"
                )
            }
            ExtractError::RateNotFound { filter } => {
                write!(f, "Rate could not be found with filter ({filter})")
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::candid::AssetClass;

    /// The function returns sample [QueriedExchangeRate] structs for testing.
    fn get_rates(
        first_asset: (String, String),
        second_asset: (String, String),
    ) -> (QueriedExchangeRate, QueriedExchangeRate) {
        (
            QueriedExchangeRate {
                base_asset: Asset {
                    symbol: first_asset.0,
                    class: AssetClass::Cryptocurrency,
                },
                quote_asset: Asset {
                    symbol: first_asset.1,
                    class: AssetClass::Cryptocurrency,
                },
                timestamp: 1661523960,
                rates: vec![123, 88, 109],
                base_asset_num_queried_sources: 3,
                base_asset_num_received_rates: 3,
                quote_asset_num_queried_sources: 2,
                quote_asset_num_received_rates: 2,
            },
            QueriedExchangeRate {
                base_asset: Asset {
                    symbol: second_asset.0,
                    class: AssetClass::Cryptocurrency,
                },
                quote_asset: Asset {
                    symbol: second_asset.1,
                    class: AssetClass::Cryptocurrency,
                },
                timestamp: 1661437560,
                rates: vec![9876, 10203, 9919, 10001],
                base_asset_num_queried_sources: 4,
                base_asset_num_received_rates: 4,
                quote_asset_num_queried_sources: 1,
                quote_asset_num_received_rates: 1,
            },
        )
    }

    /// The function verifies that that [QueriedExchangeRate] structs are multiplied correctly.
    #[test]
    fn queried_exchange_rate_multiplication() {
        let (a_b_rate, b_c_rate) = get_rates(
            ("A".to_string(), "B".to_string()),
            ("B".to_string(), "C".to_string()),
        );
        let a_c_rate = QueriedExchangeRate {
            base_asset: Asset {
                symbol: "A".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            quote_asset: Asset {
                symbol: "C".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            timestamp: 1661523960,
            rates: vec![121, 125, 122, 123, 86, 89, 87, 88, 107, 111, 108, 109],
            base_asset_num_queried_sources: 3,
            base_asset_num_received_rates: 3,
            quote_asset_num_queried_sources: 1,
            quote_asset_num_received_rates: 1,
        };
        assert_eq!(a_c_rate, a_b_rate * b_c_rate);
    }

    /// The function verifies that that [QueriedExchangeRate] structs are divided correctly.
    #[test]
    fn queried_exchange_rate_division() {
        let (a_b_rate, c_b_rate) = get_rates(
            ("A".to_string(), "B".to_string()),
            ("C".to_string(), "b".to_string()),
        );
        let a_c_rate = QueriedExchangeRate {
            base_asset: Asset {
                symbol: "A".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            quote_asset: Asset {
                symbol: "C".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            timestamp: 1661523960,
            rates: vec![124, 120, 123, 122, 89, 86, 88, 87, 110, 106, 109, 108],
            base_asset_num_queried_sources: 3,
            base_asset_num_received_rates: 3,
            quote_asset_num_queried_sources: 4,
            quote_asset_num_received_rates: 4,
        };
        assert_eq!(a_c_rate, a_b_rate / c_b_rate);
    }
}
