#![warn(missing_docs)]

//! The XRC provides a powerful exchange rate API, which can be leveraged by
//! other applications, e.g., in the DeFi space.
//! TODO: expand on this documentation

mod exchanges;
mod http;
mod jq;
mod types;

use exchanges::Exchange;
use ic_cdk::export::candid::candid_method;

use jaq_core::Val;

use http::CanisterHttpRequest;

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
    let exchanges = vec![exchanges::Coinbase::new()];
    let (rates, _errors) = exchanges::call_exchanges(&exchanges, &request).await;
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
