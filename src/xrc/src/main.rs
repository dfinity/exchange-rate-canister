use ic_cdk::api::management_canister::http_request::HttpResponse;
use ic_cdk::export::candid::candid_method;
use std::cell::Cell;
use xrc::cache::ExchangeRateCache;
use xrc::{candid, jq, CACHE_EXPIRATION_TIME_SEC, HARD_MAX_CACHE_SIZE, SOFT_MAX_CACHE_SIZE};

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

thread_local! {
    // The exchange rate cache.
    static EXCHANGE_RATE_CACHE: Cell<ExchangeRateCache> = Cell::new(
        ExchangeRateCache::new(SOFT_MAX_CACHE_SIZE, HARD_MAX_CACHE_SIZE, CACHE_EXPIRATION_TIME_SEC));
}

#[ic_cdk_macros::query]
#[candid_method(query)]
fn transform_http_response(response: HttpResponse) -> HttpResponse {
    xrc::transform_http_response(response)
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
