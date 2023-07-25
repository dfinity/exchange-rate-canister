use candid::candid_method;
use ic_cdk::api::management_canister::http_request::{HttpResponse, TransformArgs};

#[ic_cdk_macros::update]
#[candid_method(update)]
async fn get_exchange_rate(
    request: ic_xrc_types::GetExchangeRateRequest,
) -> ic_xrc_types::GetExchangeRateResult {
    xrc::get_exchange_rate(request).await
}

#[ic_cdk_macros::query]
fn transform_exchange_http_response(args: TransformArgs) -> HttpResponse {
    xrc::transform_exchange_http_response(args)
}

#[ic_cdk_macros::query]
fn transform_forex_http_response(args: TransformArgs) -> HttpResponse {
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

#[ic_cdk_macros::heartbeat]
fn heartbeat() {
    xrc::heartbeat()
}

#[ic_cdk_macros::query]
pub fn http_request(request: xrc::types::HttpRequest) -> xrc::types::HttpResponse {
    xrc::http_request(request)
}

/// Inspect ingress messages coming in to ensure that only messages from other canisters or requests
/// to the metrics are allowed.
///
/// https://internetcomputer.org/docs/current/references/ic-interface-spec/#system-api-inspect-message
#[ic_cdk_macros::inspect_message]
pub fn inspect_message() {}

fn main() {}

#[cfg(test)]
mod test {
    use std::path::PathBuf;

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
