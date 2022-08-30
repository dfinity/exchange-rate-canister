#![deny(missing_docs)]

//! The XRC provides a powerful exchange rate API, which can be leveraged by
//! other applications, e.g., in the DeFi space.
// TODO: expand on this documentation

mod exchanges;
mod http;
mod jq;
mod types;

use exchanges::{Exchange, EXCHANGES};
use ic_cdk::export::candid::candid_method;

use jaq_core::Val;

use http::CanisterHttpRequest;

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

/// TODO: Move types to candid to have a separate area where candid types are defined.
impl From<types::GetExchangeRateRequest> for CallExchangesArgs {
    fn from(request: types::GetExchangeRateRequest) -> Self {
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

#[ic_cdk_macros::query]
#[candid_method(query)]
fn greet(name: String) -> String {
    format!("Hello, {}!", name)
}

#[ic_cdk_macros::query]
#[candid_method(query)]
fn extract_rate(response: String, filter: String) -> u64 {
    let output = jq::extract(response.as_bytes(), &filter).unwrap();

    match output {
        Val::Num(rc_number) => ((*rc_number).as_f64().unwrap() * 100.0) as u64,
        _ => 0, // Return zero for now.
    }
}

#[ic_cdk_macros::update]
#[candid_method(update)]
fn get_exchange_rate(_request: types::GetExchangeRateRequest) -> types::GetExchangeRateResult {
    todo!()
}

#[ic_cdk_macros::update]
#[candid_method(update)]
async fn extract_from_http_request(url: String, filter: String) -> String {
    let payload = CanisterHttpRequest::new().get(&url).send().await.unwrap();
    jq::extract(&payload.body, &filter).unwrap().to_string()
}

#[ic_cdk_macros::update]
#[candid_method(update)]
async fn get_exchange_rates(request: types::GetExchangeRateRequest) -> Vec<u64> {
    let (rates, _errors) = call_exchanges(CallExchangesArgs::from(request)).await;
    rates
}

#[cfg(test)]
mod test {
    use std::path::PathBuf;

    use super::*;

    use ic_cdk::export::candid;

    #[test]
    fn check_candid_compatibility() {
        candid::export_service!();
        // Pull in the rust-generated interface and candid file interface.
        let new_interface = __export_service();
        let old_interface =
            PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("xrc.did");

        candid::utils::service_compatible(
            candid::utils::CandidSource::Text(&new_interface),
            candid::utils::CandidSource::File(old_interface.as_path()),
        )
        .expect("Service incompatibility found");
    }
}
