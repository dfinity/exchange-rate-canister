use ic_cdk::export::candid::candid_method;
use jaq_core::Val;
use xrc::{candid, jq};

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
fn get_exchange_rate(_request: candid::GetExchangeRateRequest) -> candid::GetExchangeRateResult {
    todo!()
}

#[ic_cdk_macros::update]
#[candid_method(update)]
async fn extract_from_http_request(url: String, filter: String) -> String {
    let payload = xrc::CanisterHttpRequest::new()
        .get(&url)
        .send()
        .await
        .unwrap();
    jq::extract(&payload.body, &filter).unwrap().to_string()
}

#[ic_cdk_macros::update]
#[candid_method(update)]
async fn get_exchange_rates(request: candid::GetExchangeRateRequest) -> Vec<u64> {
    let (rates, _errors) = xrc::call_exchanges(xrc::CallExchangesArgs::from(request)).await;
    rates
}

fn main() {}

#[cfg(test)]
mod test {
    use std::path::PathBuf;

    use super::*;

    use ic_cdk::export::candid as cdk_candid;

    #[test]
    fn check_candid_compatibility() {
        cdk_candid::export_service!();
        // Pull in the rust-generated interface and candid file interface.
        let new_interface = __export_service();
        let old_interface =
            PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("xrc.did");

        cdk_candid::utils::service_compatible(
            cdk_candid::utils::CandidSource::Text(&new_interface),
            cdk_candid::utils::CandidSource::File(old_interface.as_path()),
        )
        .expect("Service incompatibility found");
    }
}
