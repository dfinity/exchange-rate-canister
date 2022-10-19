use ic_cdk::caller;
use ic_cdk::export::candid::candid_method;
use xrc::candid;

#[ic_cdk_macros::update]
#[candid_method(update)]
async fn get_exchange_rate(
    request: candid::GetExchangeRateRequest,
) -> candid::GetExchangeRateResult {
    xrc::get_exchange_rate(caller(), request).await
}

#[ic_cdk_macros::query]
fn transform_exchange_http_response(
    args: xrc::canister_http::TransformArgs,
) -> xrc::canister_http::HttpResponse {
    xrc::transform_exchange_http_response(args)
}

#[ic_cdk_macros::query]
fn transform_forex_http_response(
    args: xrc::canister_http::TransformArgs,
) -> xrc::canister_http::HttpResponse {
    xrc::transform_forex_http_response(args)
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
