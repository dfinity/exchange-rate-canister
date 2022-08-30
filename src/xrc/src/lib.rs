#![warn(missing_docs)]

//! The XRC provides a powerful exchange rate API, which can be leveraged by
//! other applications, e.g., in the DeFi space.
// TODO: expand on this documentation

/// This module provides the candid types to be used over the wire.
pub mod candid;
mod exchanges;
mod http;

// TODO: long-term should not be public
/// This module provides the ability to use `jq` filters on the returned
/// response bodies.
pub mod jq;

use exchanges::{Exchange, EXCHANGES};

// TODO: ultimately, should not be accessible by the canister methods
pub use http::CanisterHttpRequest;
use ic_cdk::api::management_canister::http_request::HttpResponse;

/// The arguments for the [call_exchanges] function.
pub struct CallExchangesArgs {
    /// The timestamp provided by the user or the time from the IC.
    pub timestamp: u64,
    /// The asset to be used as the starting asset. For example, using
    /// ICP/USD, USD would be the quote asset.
    pub quote_asset: String,
    /// The asset to be used as the resulting asset. For example, using
    /// ICP/USD, ICP would be the base asset.
    pub base_asset: String,
}

/// The possible errors that can occur when calling an exchange.
#[derive(Debug)]
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
        error: jq::ExtractError,
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

impl From<candid::GetExchangeRateRequest> for CallExchangesArgs {
    fn from(request: candid::GetExchangeRateRequest) -> Self {
        Self {
            timestamp: request.timestamp.unwrap_or_else(ic_cdk::api::time),
            quote_asset: request.quote_asset,
            base_asset: request.base_asset,
        }
    }
}

/// This function calls all of the known exchanges and gathers all of
/// the discovered rates and received errors.
pub async fn call_exchanges(args: CallExchangesArgs) -> (Vec<u64>, Vec<CallExchangeError>) {
    let results = futures::future::join_all(
        EXCHANGES
            .iter()
            .map(|exchange| call_exchange(exchange, &args)),
    )
    .await;
    let mut rates = vec![];
    let mut errors = vec![];
    for result in results {
        match result {
            Ok(rate) => rates.push(rate),
            Err(error) => errors.push(error),
        }
    }
    (rates, errors)
}

async fn call_exchange(
    exchange: &Exchange,
    args: &CallExchangesArgs,
) -> Result<u64, CallExchangeError> {
    let url = exchange.get_url(&args.base_asset, &args.quote_asset, args.timestamp);
    let response = CanisterHttpRequest::new()
        .get(&url)
        .send()
        .await
        .map_err(|error| CallExchangeError::Http {
            exchange: exchange.to_string(),
            error,
        })?;

    exchange
        .extract_rate(&response.body, args.timestamp)
        .map_err(|error| CallExchangeError::Extract {
            exchange: exchange.to_string(),
            error,
        })
}

/// This function sanitizes the [HttpResponse] as requests must be idempotent.
/// Currently, this function strips out the response headers as that is the most
/// likely culprit to cause issues.
///
/// [Interface Spec - IC method `http_request`](https://internetcomputer.org/docs/current/references/ic-interface-spec/#ic-http_request)
pub fn transform_http_request(response: HttpResponse) -> HttpResponse {
    let mut sanitized = response;
    // Strip out the headers as these will commonly cause an error to occur.
    sanitized.headers = vec![];
    sanitized
}
