#![warn(missing_docs)]

//! The exchange rate canister (XRC) provides an oracle service for cryptocurrency and fiat currency
//! exchange rates.
//!
//! Canisters can interact with the exchange rate canister through the [get_exchange_rate] endpoint.

mod api;
mod cache;
mod exchanges;
mod forex;
mod http;
mod stablecoin;

mod environment;
mod errors;
mod inflight;
mod periodic;
mod rate_limiting;
mod request_log;
/// This module provides types for responding to HTTP requests for metrics.
pub mod types;
mod utils;

use ::candid::{CandidType, Deserialize};
use cache::ExchangeRateCache;
use ic_cdk::{
    api::management_canister::http_request::{HttpResponse, TransformArgs},
    export::candid::Principal,
};
use ic_xrc_types::{Asset, ExchangeRate, ExchangeRateError, ExchangeRateMetadata, OtherError};
use request_log::RequestLog;
use serde_bytes::ByteBuf;

use crate::forex::ForexRateStore;
use forex::{Forex, ForexContextArgs, ForexRateMap, ForexRatesCollector, FOREX_SOURCES};
use http::CanisterHttpRequest;
use std::{
    cell::{Cell, RefCell},
    mem::{size_of, size_of_val},
};

use crate::errors::{INVALID_RATE_ERROR_CODE, INVALID_RATE_ERROR_MESSAGE};
pub use api::get_exchange_rate;
pub use api::usdt_asset;
pub use exchanges::{Exchange, EXCHANGES};
use utils::{median, standard_deviation};

/// Rates may not deviate by more than one tenth of the smallest considered rate.
const RATE_DEVIATION_DIVISOR: u64 = 10;

const LOG_PREFIX: &str = "[xrc]";

/// The number of cycles needed to use the `xrc` canister.
pub const XRC_REQUEST_CYCLES_COST: u64 = 10_000_000_000;

/// The cost in cycles needed to make an outbound HTTP call.
pub const XRC_OUTBOUND_HTTP_CALL_CYCLES_COST: u64 = 2_400_000_000;

/// The amount of cycles refunded off the top of a call. Number will be adjusted based
/// on the number of sources the canister will use.
pub const XRC_IMMEDIATE_REFUND_CYCLES: u64 = 5_000_000_000;

/// The base cost in cycles that will always be charged when receiving a valid response from the `xrc` canister.
pub const XRC_BASE_CYCLES_COST: u64 = 200_000_000;

/// The amount of cycles charged if a call fails (rate limited, failed to find forex rate in store, etc.).
pub const XRC_MINIMUM_FEE_COST: u64 = 10_000_000;

const PRIVILEGED_CANISTER_IDS: [Principal; 3] = [
    // CMC: rkp4c-7iaaa-aaaaa-aaaca-cai
    Principal::from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x01, 0x01]),
    // NNS dapp: qoctq-giaaa-aaaaa-aaaea-cai
    Principal::from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x01, 0x01]),
    // TVL dapp: ewh3f-3qaaa-aaaap-aazjq-cai
    Principal::from_slice(&[0x00, 0x00, 0x00, 0x00, 0x01, 0xe0, 0x06, 0x53, 0x01, 0x01]),
];

/// The currency symbol for the US dollar.
const USD: &str = "USD";

/// The symbol for the USDT stablecoin.
const USDT: &str = "USDT";

/// The symbol for the Dai stablecoin.
const DAI: &str = "DAI";

/// The symbol for the USDC stablecoin.
const USDC: &str = "USDC";

/// The maximum size of the cache.
const MAX_CACHE_SIZE: usize = 1000;

/// 9 decimal places are used for rates and standard deviations.
const DECIMALS: u32 = 9;

/// The rate unit is 10^DECIMALS.
const RATE_UNIT: u64 = 10u64.saturating_pow(DECIMALS);

/// Used for setting the max response bytes for the exchanges and forexes.
const ONE_KIB: u64 = 1_024;

/// 1 minute in seconds
const ONE_MINUTE_SECONDS: u64 = 60;
// 1 hour in seconds
const ONE_HOUR_SECONDS: u64 = 60 * ONE_MINUTE_SECONDS;
// 1 day in seconds
const ONE_DAY_SECONDS: u64 = 24 * ONE_HOUR_SECONDS;

/// Maximum number of entries in the privileged request log.
const MAX_PRIVILEGED_REQUEST_LOG_ENTRIES: usize = 100;

/// Maximum number of entries in the non-privileged request log.
const MAX_NONPRIVILEGED_REQUEST_LOG_ENTRIES: usize = 50;

thread_local! {
    // The exchange rate cache.
    static EXCHANGE_RATE_CACHE: RefCell<ExchangeRateCache> = RefCell::new(
        ExchangeRateCache::new(MAX_CACHE_SIZE));

    static FOREX_RATE_STORE: RefCell<ForexRateStore> = RefCell::new(ForexRateStore::new());
    static FOREX_RATE_COLLECTOR: RefCell<ForexRatesCollector> = RefCell::new(ForexRatesCollector::new());

    /// A simple structure to collect privileged canister requests and responses.
    static PRIVILEGED_REQUEST_LOG: RefCell<RequestLog> = RefCell::new(RequestLog::new(MAX_PRIVILEGED_REQUEST_LOG_ENTRIES));
    /// A simple structure to collect non-privileged canister requests and responses.
    static NONPRIVILEGED_REQUEST_LOG: RefCell<RequestLog> = RefCell::new(RequestLog::new(MAX_NONPRIVILEGED_REQUEST_LOG_ENTRIES));

    /// The counter used to determine if a request should be rate limited or not.
    static RATE_LIMITING_REQUEST_COUNTER: Cell<usize> = Cell::new(0);

    /// Metrics
    static GET_EXCHANGE_RATE_REQUEST_COUNTER: Cell<usize> = Cell::new(0);
    static GET_EXCHANGE_RATE_REQUEST_FROM_CMC_COUNTER: Cell<usize> = Cell::new(0);
    static CYCLE_RELATED_ERRORS_COUNTER: Cell<usize> = Cell::new(0);
    static ERRORS_RETURNED_COUNTER: Cell<usize> = Cell::new(0);
    static ERRORS_RETURNED_TO_CMC_COUNTER: Cell<usize> = Cell::new(0);
    static PENDING_ERRORS_RETURNED_COUNTER: Cell<usize> = Cell::new(0);
    static RATE_LIMITING_ERRORS_RETURNED_COUNTER: Cell<usize> = Cell::new(0);
    static STABLECOIN_ERRORS_RETURNED_COUNTER: Cell<usize> = Cell::new(0);
    static INCONSISTENT_RATES_ERROR_COUNTER: Cell<usize> = Cell::new(0);
    static CRYPTO_ASSET_RELATED_ERRORS_COUNTER: Cell<usize> = Cell::new(0);
    static FOREX_ASSET_RELATED_ERRORS_COUNTER: Cell<usize> = Cell::new(0);

}

/// Used to retrieve or increment the various metric counters in the state.
enum MetricCounter {
    /// Maps to the [GET_EXCHANGE_RATE_REQUEST_COUNTER].
    GetExchangeRateRequest,
    /// Maps to the [GET_EXCHANGE_RATE_REQUEST_FROM_CMC_COUNTER].
    GetExchangeRateRequestFromCmc,
    /// Maps to the [ERRORS_RETURNED_COUNTER].
    ErrorsReturned,
    /// Maps to the [RATE_LIMITING_ERRORS_RETURNED_COUNTER].
    RateLimitedErrors,
    /// Maps to the [CYCLE_RELATED_ERRORS_COUNTER].
    CycleRelatedErrors,
    /// Maps to the [PENDING_ERRORS_RETURNED_COUNTER].
    PendingErrorsReturned,
    /// Maps to the [STABLECOIN_ERRORS_RETURNED_COUNTER].
    StablecoinErrorsReturned,
    /// Maps to the [INCONSISTENT_RATES_ERROR_COUNTER].
    InconsistentRatesErrorsReturned,
    /// Maps to the [CRYPTO_ASSET_RELATED_ERRORS_COUNTER].
    CryptoAssetRelatedErrorsReturned,
    /// Maps to the [FOREX_ASSET_RELATED_ERRORS_COUNTER].
    ForexAssetRelatedErrorsReturned,
    /// Maps to the [ERRORS_RETURNED_TO_CMC_COUNTER].
    ErrorsReturnedToCmc,
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
            MetricCounter::ErrorsReturnedToCmc => ERRORS_RETURNED_TO_CMC_COUNTER.with(|c| c.get()),
            MetricCounter::PendingErrorsReturned => {
                PENDING_ERRORS_RETURNED_COUNTER.with(|c| c.get())
            }
            MetricCounter::RateLimitedErrors => {
                RATE_LIMITING_ERRORS_RETURNED_COUNTER.with(|c| c.get())
            }
            MetricCounter::StablecoinErrorsReturned => {
                STABLECOIN_ERRORS_RETURNED_COUNTER.with(|c| c.get())
            }
            MetricCounter::InconsistentRatesErrorsReturned => {
                INCONSISTENT_RATES_ERROR_COUNTER.with(|c| c.get())
            }
            MetricCounter::CryptoAssetRelatedErrorsReturned => {
                CRYPTO_ASSET_RELATED_ERRORS_COUNTER.with(|c| c.get())
            }
            MetricCounter::ForexAssetRelatedErrorsReturned => {
                FOREX_ASSET_RELATED_ERRORS_COUNTER.with(|c| c.get())
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
            MetricCounter::ErrorsReturnedToCmc => {
                ERRORS_RETURNED_TO_CMC_COUNTER.with(|c| c.set(c.get().saturating_add(1)));
            }
            MetricCounter::PendingErrorsReturned => {
                PENDING_ERRORS_RETURNED_COUNTER.with(|c| c.set(c.get().saturating_add(1)))
            }
            MetricCounter::RateLimitedErrors => {
                RATE_LIMITING_ERRORS_RETURNED_COUNTER.with(|c| c.set(c.get().saturating_add(1)))
            }
            MetricCounter::StablecoinErrorsReturned => {
                STABLECOIN_ERRORS_RETURNED_COUNTER.with(|c| c.set(c.get().saturating_add(1)))
            }
            MetricCounter::InconsistentRatesErrorsReturned => {
                INCONSISTENT_RATES_ERROR_COUNTER.with(|c| c.set(c.get().saturating_add(1)))
            }
            MetricCounter::CryptoAssetRelatedErrorsReturned => {
                CRYPTO_ASSET_RELATED_ERRORS_COUNTER.with(|c| c.set(c.get().saturating_add(1)))
            }
            MetricCounter::ForexAssetRelatedErrorsReturned => {
                FOREX_ASSET_RELATED_ERRORS_COUNTER.with(|c| c.set(c.get().saturating_add(1)))
            }
        }
    }
}

/// A trait used to indicate the size in bytes a particular object contains.
trait AllocatedBytes {
    /// Returns the amount of memory in bytes that has been allocated.
    fn allocated_bytes(&self) -> usize {
        0
    }
}

impl AllocatedBytes for Asset {
    fn allocated_bytes(&self) -> usize {
        size_of::<Self>() + self.symbol.len()
    }
}

fn with_cache<R>(f: impl FnOnce(&ExchangeRateCache) -> R) -> R {
    EXCHANGE_RATE_CACHE.with(|cache| f(&cache.borrow()))
}

fn with_cache_mut<R>(f: impl FnOnce(&mut ExchangeRateCache) -> R) -> R {
    EXCHANGE_RATE_CACHE.with(|cache| f(&mut cache.borrow_mut()))
}

/// A helper method to read from the forex rate store.
fn with_forex_rate_store<R>(f: impl FnOnce(&ForexRateStore) -> R) -> R {
    FOREX_RATE_STORE.with(|cell| f(&cell.borrow()))
}

/// A helper method to mutate the forex rate store.
#[allow(dead_code)]
fn with_forex_rate_store_mut<R>(f: impl FnOnce(&mut ForexRateStore) -> R) -> R {
    FOREX_RATE_STORE.with(|cell| f(&mut cell.borrow_mut()))
}

/// A helper method to read from the forex rate collector.
fn with_forex_rate_collector<R>(f: impl FnOnce(&ForexRatesCollector) -> R) -> R {
    FOREX_RATE_COLLECTOR.with(|cell| f(&cell.borrow()))
}

/// A helper method to mutate the forex rate collector.
fn with_forex_rate_collector_mut<R>(f: impl FnOnce(&mut ForexRatesCollector) -> R) -> R {
    FOREX_RATE_COLLECTOR.with(|cell| f(&mut cell.borrow_mut()))
}

/// The received rates for a particular exchange rate request are stored in this struct.
#[derive(CandidType, Deserialize, Clone, Debug, PartialEq)]
pub(crate) struct QueriedExchangeRate {
    /// The base asset.
    pub base_asset: Asset,
    /// The quote asset.
    pub quote_asset: Asset,
    /// The timestamp associated with the returned rate.
    pub timestamp: u64,
    /// The received rates scaled by `RATE_UNIT`.
    pub rates: Vec<u64>,
    /// The median rate scaled by `RATE_UNIT`, if there is a valid rate.
    pub median_rate: Option<u64>,
    /// The number of queried exchanges for the base asset.
    pub base_asset_num_queried_sources: usize,
    /// The number of rates successfully received from the queried sources for the quote asset.
    pub base_asset_num_received_rates: usize,
    /// The number of queried exchanges for the base asset.
    pub quote_asset_num_queried_sources: usize,
    /// The number of rates successfully received from the queried sources for the quote asset.
    pub quote_asset_num_received_rates: usize,
    /// The timestamp of the beginning of the day for which the forex rates were retrieved, if any.
    pub forex_timestamp: Option<u64>,
}

impl Default for QueriedExchangeRate {
    fn default() -> Self {
        Self {
            base_asset: usdt_asset(),
            quote_asset: usdt_asset(),
            timestamp: Default::default(),
            rates: Default::default(),
            median_rate: Default::default(),
            base_asset_num_queried_sources: Default::default(),
            base_asset_num_received_rates: Default::default(),
            quote_asset_num_queried_sources: Default::default(),
            quote_asset_num_received_rates: Default::default(),
            forex_timestamp: None,
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
        let median_rate = match (self.median_rate, other_rate.median_rate) {
            (Some(own_rate), Some(other_rate)) => {
                let product_rate = (own_rate as u128)
                    .saturating_mul(other_rate as u128)
                    .saturating_div(RATE_UNIT as u128);
                if product_rate <= (RATE_UNIT * RATE_UNIT) as u128 {
                    Some(product_rate as u64)
                } else {
                    None
                }
            }
            (_, _) => None,
        };

        // Return a result with empty rates if there is no median.
        if median_rate.is_none() {
            return Self {
                base_asset: self.base_asset,
                quote_asset: other_rate.quote_asset,
                timestamp: self.timestamp,
                rates: vec![],
                median_rate,
                base_asset_num_queried_sources: self.base_asset_num_queried_sources,
                base_asset_num_received_rates: self.base_asset_num_received_rates,
                quote_asset_num_queried_sources: other_rate.quote_asset_num_queried_sources,
                quote_asset_num_received_rates: other_rate.quote_asset_num_received_rates,
                forex_timestamp: None,
            };
        }

        let forex_timestamp = match (self.forex_timestamp, other_rate.forex_timestamp) {
            (None, Some(timestamp)) | (Some(timestamp), None) => Some(timestamp),
            (Some(self_timestamp), Some(other_timestamp)) => {
                if self_timestamp == other_timestamp {
                    Some(self_timestamp)
                } else {
                    None
                }
            }
            (None, None) => None,
        };

        let mut rates = vec![];
        for own_value in self.rates {
            // Convert to a u128 to avoid the rate being saturated.
            let own_value = own_value as u128;
            for other_value in other_rate.rates.iter() {
                let other_value = *other_value as u128;
                let rate = own_value
                    .saturating_mul(other_value)
                    .saturating_div(RATE_UNIT as u128);
                // The rate must not exceed RATE_UNIT * RATE_UNIT.
                if rate <= (RATE_UNIT * RATE_UNIT) as u128 {
                    rates.push(rate as u64);
                }
            }
        }
        rates.sort();
        Self {
            base_asset: self.base_asset,
            quote_asset: other_rate.quote_asset,
            timestamp: self.timestamp,
            rates,
            median_rate,
            base_asset_num_queried_sources: self.base_asset_num_queried_sources,
            base_asset_num_received_rates: self.base_asset_num_received_rates,
            quote_asset_num_queried_sources: other_rate.quote_asset_num_queried_sources,
            quote_asset_num_received_rates: other_rate.quote_asset_num_received_rates,
            forex_timestamp,
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

impl AllocatedBytes for QueriedExchangeRate {
    fn allocated_bytes(&self) -> usize {
        self.base_asset.allocated_bytes()
            + self.quote_asset.allocated_bytes()
            + size_of_val(&self.base_asset_num_queried_sources)
            + size_of_val(&self.base_asset_num_received_rates)
            + size_of_val(&self.quote_asset_num_queried_sources)
            + size_of_val(&self.quote_asset_num_received_rates)
            + size_of_val(&self.timestamp)
            + self.rates.allocated_bytes()
    }
}

impl AllocatedBytes for Vec<u64> {
    fn allocated_bytes(&self) -> usize {
        size_of_val(self) + (self.len() * size_of::<u64>())
    }
}

impl TryFrom<QueriedExchangeRate> for ExchangeRate {
    type Error = ExchangeRateError;

    fn try_from(rate: QueriedExchangeRate) -> Result<Self, Self::Error> {
        match rate.median_rate {
            Some(median_rate) => {
                // If no rates were invalid,the median over all rates can be used.
                let used_median = if rate.rates.len()
                    == rate.base_asset_num_received_rates * rate.quote_asset_num_received_rates
                {
                    median(&rate.rates)
                } else {
                    median_rate
                };
                Ok(ExchangeRate {
                    base_asset: rate.base_asset,
                    quote_asset: rate.quote_asset,
                    timestamp: rate.timestamp,
                    rate: used_median,
                    metadata: ExchangeRateMetadata {
                        decimals: DECIMALS,
                        base_asset_num_queried_sources: rate.base_asset_num_queried_sources,
                        base_asset_num_received_rates: rate.base_asset_num_received_rates,
                        quote_asset_num_queried_sources: rate.quote_asset_num_queried_sources,
                        quote_asset_num_received_rates: rate.quote_asset_num_received_rates,
                        standard_deviation: standard_deviation(&rate.rates),
                        forex_timestamp: rate.forex_timestamp,
                    },
                })
            }
            None => Err(ExchangeRateError::Other(OtherError {
                code: INVALID_RATE_ERROR_CODE,
                description: INVALID_RATE_ERROR_MESSAGE.to_string(),
            })),
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
        forex_timestamp: Option<u64>,
    ) -> QueriedExchangeRate {
        let mut rates = rates.to_vec();
        // Filter out rates that are 0, which are invalid, or greater than RATE_UNIT * RATE_UNIT,
        // which cannot be inverted.
        rates = rates
            .into_iter()
            .filter(|rate| *rate > 0 && *rate <= RATE_UNIT * RATE_UNIT)
            .collect();
        rates.sort();
        // More than half of the received rates must be valid.
        let (rates, median_rate) = if rates.len() > num_received_rates / 2 {
            let median = median(&rates);
            (rates, Some(median))
        } else {
            (vec![], None)
        };

        Self {
            base_asset,
            quote_asset,
            timestamp,
            rates,
            median_rate,
            base_asset_num_queried_sources: num_queried_sources,
            base_asset_num_received_rates: num_received_rates,
            quote_asset_num_queried_sources: num_queried_sources,
            quote_asset_num_received_rates: num_received_rates,
            forex_timestamp,
        }
    }

    /// The function returns the exchange rate with base asset and quote asset inverted.
    pub(crate) fn inverted(&self) -> Self {
        let median_rate = match self.median_rate {
            Some(rate) => utils::checked_invert_rate(rate),
            None => None,
        };

        let mut inverted_rates: Vec<_> = self
            .rates
            .iter()
            .filter_map(|rate| utils::checked_invert_rate(*rate))
            .collect();
        inverted_rates.sort();
        Self {
            base_asset: self.quote_asset.clone(),
            quote_asset: self.base_asset.clone(),
            timestamp: self.timestamp,
            rates: inverted_rates,
            median_rate,
            base_asset_num_queried_sources: self.quote_asset_num_queried_sources,
            base_asset_num_received_rates: self.quote_asset_num_received_rates,
            quote_asset_num_queried_sources: self.base_asset_num_queried_sources,
            quote_asset_num_received_rates: self.base_asset_num_received_rates,
            forex_timestamp: self.forex_timestamp,
        }
    }

    /// The function checks that the relative deviation among sufficiently many rates does
    /// not exceed 100/[RATE_DEVIATION_DIVISOR] percent.
    fn is_valid(&self) -> bool {
        let num = self.rates.len();
        let diff = num / 2;
        for end in diff..num {
            if self.rates[end] - self.rates[end - diff]
                <= self.rates[end - diff].saturating_div(RATE_DEVIATION_DIVISOR)
            {
                return true;
            }
        }
        false
    }
}

/// The arguments for the [call_exchange] function.
#[derive(Clone)]
pub(crate) struct CallExchangeArgs {
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
                write!(f, "Failed to retrieve rate from {exchange}: {error}")
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

impl From<ic_xrc_types::GetExchangeRateRequest> for CallExchangeArgs {
    fn from(request: ic_xrc_types::GetExchangeRateRequest) -> Self {
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
        .max_response_bytes(exchange.max_response_bytes())
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
                write!(f, "Failed to retrieve rates from {forex}: {error}")
            }
            CallForexError::Candid { forex, error } => {
                write!(f, "Failed to encode/decode {forex}: {error}")
            }
        }
    }
}

/// Function used to call a single forex with a set of arguments.
async fn call_forex(forex: &Forex, args: ForexContextArgs) -> Result<ForexRateMap, CallForexError> {
    let url = forex.get_url(args.timestamp);
    let context = forex
        .encode_context(&args)
        .map_err(|error| CallForexError::Candid {
            forex: forex.to_string(),
            error: error.to_string(),
        })?;

    let response = CanisterHttpRequest::new()
        .get(&url)
        .transform_context("transform_forex_http_response", context)
        .max_response_bytes(forex.max_response_bytes())
        .add_headers(forex.get_additional_http_request_headers())
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

    let index = match Exchange::decode_context(&args.context) {
        Ok(index) => index,
        Err(err) => ic_cdk::trap(&format!("Failed to decode context: {}", err)),
    };

    // It should be ok to trap here as this does not modify state.
    let exchange = match EXCHANGES.get(index) {
        Some(exchange) => exchange,
        None => {
            ic_cdk::trap(&format!(
                "Provided index {} does not map to any supported exchange.",
                index
            ));
        }
    };

    let rate = match exchange.extract_rate(&sanitized.body) {
        Ok(rate) => rate,
        Err(err) => {
            if let Exchange::KuCoin(_) = exchange {
                ic_cdk::println!("{} [KuCoin] {}", LOG_PREFIX, err);
            }

            ic_cdk::trap(&format!("{}", err));
        }
    };

    sanitized.body = match Exchange::encode_response(rate) {
        Ok(body) => body,
        Err(err) => {
            ic_cdk::trap(&format!("failed to encode rate ({}): {}", rate, err));
        }
    };

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
    let context = match Forex::decode_context(&args.context) {
        Ok(context) => context,
        Err(err) => {
            ic_cdk::println!("Failed to decode context: {}", err);
            ic_cdk::trap(&format!("Failed to decode context: {}", err));
        }
    };

    let forex = match FOREX_SOURCES.get(context.id) {
        Some(forex) => forex,
        None => {
            ic_cdk::println!(
                "Provided forex index {} does not map to any supported forex source.",
                context.id
            );
            ic_cdk::trap(&format!(
                "Provided forex index {} does not map to any supported forex source.",
                context.id
            ));
        }
    };

    let transform_result = forex.transform_http_response_body(&sanitized.body, &context.payload);

    sanitized.body = match transform_result {
        Ok(body) => body,
        Err(err) => {
            ic_cdk::trap(&format!("{}", err));
        }
    };

    // Strip out the headers as these will commonly cause an error to occur.
    sanitized.headers = vec![];
    sanitized
}

/// This adds the ability to handle HTTP requests to the canister.
/// Used to expose metrics to prometheus.
pub fn http_request(req: types::HttpRequest) -> types::HttpResponse {
    let parts: Vec<&str> = req.url.split('?').collect();
    match parts[0] {
        "/dashboard" => api::get_dashboard(),
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
    JsonDeserialize {
        /// The actual response from the request.
        response: String,
        /// Deserialization error from serde.
        error: String,
    },
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
    Extract(String),
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
            ExtractError::Extract(response) => {
                write!(f, "Failed to extract rate from response: {}", response)
            }
            ExtractError::JsonDeserialize { error, response } => {
                write!(
                    f,
                    "Failed to deserialize JSON: error: {error} response: {response}"
                )
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

impl ExtractError {
    fn json_deserialize(bytes: &[u8], error: String) -> Self {
        let response = String::from_utf8(bytes.to_vec())
            .unwrap_or_default()
            .replace('\n', " ");

        Self::JsonDeserialize { response, error }
    }

    fn extract(bytes: &[u8]) -> Self {
        let response = String::from_utf8(bytes.to_vec())
            .unwrap_or_default()
            .replace('\n', " ");

        Self::Extract(response)
    }
}

#[cfg(test)]
mod test {

    use ic_xrc_types::AssetClass;

    use super::*;

    /// The function returns sample [QueriedExchangeRate] structs for testing.
    fn get_rates(
        first_asset: (String, String),
        second_asset: (String, String),
    ) -> (QueriedExchangeRate, QueriedExchangeRate) {
        (
            QueriedExchangeRate::new(
                Asset {
                    symbol: first_asset.0,
                    class: AssetClass::Cryptocurrency,
                },
                Asset {
                    symbol: first_asset.1,
                    class: AssetClass::Cryptocurrency,
                },
                1661523960,
                &[8_800_000, 10_900_000, 12_300_000],
                3,
                3,
                None,
            ),
            QueriedExchangeRate::new(
                Asset {
                    symbol: second_asset.0,
                    class: AssetClass::Cryptocurrency,
                },
                Asset {
                    symbol: second_asset.1,
                    class: AssetClass::Cryptocurrency,
                },
                1661437560,
                &[987_600_000, 991_900_000, 1_000_100_000, 1_020_300_000],
                4,
                4,
                None,
            ),
        )
    }

    /// The function verifies that that [QueriedExchangeRate] structs are multiplied correctly.
    #[test]
    fn queried_exchange_rate_multiplication() {
        let (a_b_rate, b_c_rate) = get_rates(
            ("A".to_string(), "B".to_string()),
            ("B".to_string(), "C".to_string()),
        );
        let rates = vec![
            8_690_880, 8_728_720, 8_800_880, 8_978_640, 10_764_840, 10_811_710, 10_901_090,
            11_121_270, 12_147_480, 12_200_370, 12_301_230, 12_549_690,
        ];
        let median_rate = Some(median(&rates));
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
            rates,
            median_rate,
            base_asset_num_queried_sources: 3,
            base_asset_num_received_rates: 3,
            quote_asset_num_queried_sources: 4,
            quote_asset_num_received_rates: 4,
            forex_timestamp: None,
        };

        assert_eq!(a_c_rate, a_b_rate * b_c_rate);
    }

    /// The function verifies that when [QueriedExchangeRate] structs are multiplied the forex timestamp
    /// is carried over properly.
    #[test]
    fn queried_exchange_rate_multiplication_forex_timestamp_check() {
        let (mut a_b_rate, b_c_rate) = get_rates(
            ("A".to_string(), "B".to_string()),
            ("B".to_string(), "C".to_string()),
        );

        a_b_rate.forex_timestamp = Some(1);
        let a_c_rate = a_b_rate * b_c_rate;

        assert!(matches!(a_c_rate.forex_timestamp, Some(n) if n == 1));

        let (a_b_rate, mut b_c_rate) = get_rates(
            ("A".to_string(), "B".to_string()),
            ("B".to_string(), "C".to_string()),
        );

        b_c_rate.forex_timestamp = Some(1);
        let a_c_rate = a_b_rate * b_c_rate;

        assert!(matches!(a_c_rate.forex_timestamp, Some(n) if n == 1));

        let (mut a_b_rate, mut b_c_rate) = get_rates(
            ("A".to_string(), "B".to_string()),
            ("B".to_string(), "C".to_string()),
        );

        a_b_rate.forex_timestamp = Some(1);
        b_c_rate.forex_timestamp = Some(1);
        let a_c_rate = a_b_rate * b_c_rate;
        assert!(matches!(a_c_rate.forex_timestamp, Some(n) if n == 1));

        let (mut a_b_rate, mut b_c_rate) = get_rates(
            ("A".to_string(), "B".to_string()),
            ("B".to_string(), "C".to_string()),
        );

        a_b_rate.forex_timestamp = Some(1);
        b_c_rate.forex_timestamp = Some(2);
        let a_c_rate = a_b_rate * b_c_rate;
        assert!(matches!(a_c_rate.forex_timestamp, None));
    }

    /// The function verifies that that [QueriedExchangeRate] structs are divided correctly.
    #[test]
    fn queried_exchange_rate_division() {
        let (a_b_rate, c_b_rate) = get_rates(
            ("A".to_string(), "B".to_string()),
            ("C".to_string(), "B".to_string()),
        );
        let rates = vec![
            8_624_914, 8_799_120, 8_871_862, 8_910_490, 10_683_132, 10_898_910, 10_989_010,
            11_036_857, 12_055_277, 12_298_770, 12_400_443, 12_454_434,
        ];
        // The median of `a_b_rate` divided by the median of `c_b_rate`:
        let median_rate = Some(10_943_775);
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
            rates,
            median_rate,
            base_asset_num_queried_sources: 3,
            base_asset_num_received_rates: 3,
            quote_asset_num_queried_sources: 4,
            quote_asset_num_received_rates: 4,
            forex_timestamp: None,
        };
        assert_eq!(a_c_rate, a_b_rate / c_b_rate);
    }

    /// The function verifies that the validity of a [QueriedExchangeRate] struct can be checked correctly.
    #[test]
    fn queried_exchange_rate_validity() {
        let (first_rate, second_rate) = get_rates(
            ("A".to_string(), "B".to_string()),
            ("C".to_string(), "B".to_string()),
        );
        assert!(!first_rate.is_valid());
        assert!(second_rate.is_valid());

        // A rate is modified manually to test validity.
        let mut modified_rate = second_rate;
        let length = modified_rate.rates.len();
        // If one value is arbitrarily large, the rate is still valid.
        modified_rate.rates[length - 1] = 1_000_000_000_000;
        assert!(modified_rate.is_valid());
        modified_rate.rates[0] = 0;
        // If 2 out of 4 rates are off, the rates are invalid.
        assert!(!modified_rate.is_valid());
        // If one value is arbitrarily small, the rate is still valid.
        modified_rate.rates[length - 1] = 1_020_300_000;
        assert!(modified_rate.is_valid());
    }

    #[test]
    fn zeroes_are_filtered_out_when_queried_exchange_rate_is_inverted() {
        let (a_b_rate, mut c_b_rate) = get_rates(
            ("A".to_string(), "B".to_string()),
            ("C".to_string(), "B".to_string()),
        );
        c_b_rate.rates = vec![0, 991_900_000, 1_000_100_000, 1_020_300_000];
        c_b_rate.median_rate = Some(1_000_100_000);

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
            rates: vec![
                8_624_914, 8_799_120, 8_871_862, 10_683_132, 10_898_910, 10_989_010, 12_055_277,
                12_298_770, 12_400_443,
            ],
            median_rate: Some(10898910),
            base_asset_num_queried_sources: 3,
            base_asset_num_received_rates: 3,
            quote_asset_num_queried_sources: 4,
            quote_asset_num_received_rates: 4,
            forex_timestamp: None,
        };

        assert_eq!(a_c_rate, a_b_rate / c_b_rate);
    }

    /// The function verifies that only valid rates are retained when creating
    /// a [QueriedExchangeRate] struct.
    #[test]
    fn check_valid_rates_in_queried_exchange_rate() {
        let rates = vec![
            0,
            8,
            0,
            RATE_UNIT,
            1_000_000,
            RATE_UNIT * RATE_UNIT,
            0,
            RATE_UNIT * RATE_UNIT + 1,
        ];
        let base_asset = Asset {
            symbol: "base".to_string(),
            class: AssetClass::Cryptocurrency,
        };
        let quote_asset = Asset {
            symbol: "quote".to_string(),
            class: AssetClass::Cryptocurrency,
        };
        let queried_exchange_rate = QueriedExchangeRate::new(
            base_asset.clone(),
            quote_asset.clone(),
            0,
            &rates,
            0,
            0,
            None,
        );
        assert_eq!(
            queried_exchange_rate.rates,
            vec![8, 1_000_000, RATE_UNIT, RATE_UNIT * RATE_UNIT]
        );

        let rates = vec![
            0,
            0,
            0,
            0,
            RATE_UNIT * RATE_UNIT + 1,
            RATE_UNIT * RATE_UNIT + 1,
        ];
        let exchange_rate: Result<ExchangeRate, ExchangeRateError> =
            QueriedExchangeRate::new(base_asset, quote_asset, 0, &rates, 0, 0, None).try_into();
        assert!(matches!(exchange_rate, Err(ExchangeRateError::Other(_))));
    }

    /// The function verifies that multiplying and dividing [QueriedExchangeRate] structs
    /// with rates at the limits results in errors when attempting to convert to [ExchangeRate] structs.
    #[test]
    fn conversion_to_exchange_rate_fails_when_rates_exceed_limits() {
        let small_rates = vec![1, 1, 1, RATE_UNIT];
        let base_asset = Asset {
            symbol: "base".to_string(),
            class: AssetClass::Cryptocurrency,
        };
        let quote_asset = Asset {
            symbol: "quote".to_string(),
            class: AssetClass::Cryptocurrency,
        };
        let small_queried_exchange_rate = QueriedExchangeRate::new(
            base_asset.clone(),
            quote_asset.clone(),
            0,
            &small_rates,
            small_rates.len(),
            small_rates.len(),
            None,
        );

        let large_rates = vec![1, RATE_UNIT * RATE_UNIT, RATE_UNIT * RATE_UNIT];
        let large_queried_exchange_rate = QueriedExchangeRate::new(
            base_asset,
            quote_asset,
            0,
            &large_rates,
            large_rates.len(),
            large_rates.len(),
            None,
        );
        let exchange_rate: Result<ExchangeRate, ExchangeRateError> =
            (large_queried_exchange_rate.clone() * large_queried_exchange_rate.clone()).try_into();
        assert!(matches!(
            exchange_rate,
            Err(ExchangeRateError::Other(OtherError { code, description })) if code == errors::INVALID_RATE_ERROR_CODE && description == errors::INVALID_RATE_ERROR_MESSAGE
        ));

        let exchange_rate: Result<ExchangeRate, ExchangeRateError> =
            (large_queried_exchange_rate / small_queried_exchange_rate).try_into();
        assert!(matches!(
            exchange_rate,
            Err(ExchangeRateError::Other(OtherError { code, description })) if code == errors::INVALID_RATE_ERROR_CODE && description == errors::INVALID_RATE_ERROR_MESSAGE
        ));
    }
}
