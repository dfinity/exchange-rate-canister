use ic_cdk::export::candid::candid_method;
use ic_cdk::{api::management_canister::http_request::HttpResponse, caller};
use xrc::candid;

#[ic_cdk_macros::update]
#[candid_method(update)]
async fn get_exchange_rate(
    request: candid::GetExchangeRateRequest,
) -> candid::GetExchangeRateResult {
    xrc::get_exchange_rate(caller(), request).await
}

#[ic_cdk_macros::query]
#[candid_method(query)]
fn transform_http_response(response: HttpResponse) -> HttpResponse {
    xrc::transform_http_response(response)
}

#[ic_cdk_macros::init]
fn init() {
    xrc::init();
}

#[ic_cdk_macros::pre_upgrade]
fn pre_upgrade() {
    xrc::pre_upgrade();
}

#[ic_cdk_macros::post_upgrade]
fn post_upgrade() {
    xrc::post_upgrade();
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
