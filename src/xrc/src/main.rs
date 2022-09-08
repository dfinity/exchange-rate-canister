use std::collections::BTreeMap;
use ic_cdk::api::management_canister::http_request::HttpResponse;
use ic_cdk::api::stable;
use ic_cdk::export::candid::{candid_method, CandidType, Encode, encode_one, decode_one};
use ic_cdk::print;
use xrc::{candid, jq};
use xrc::candid::{Asset, ExchangeRateInformation};
use serde::{Deserialize, Serialize};
use lazy_static::lazy_static;
use std::sync::RwLock;
use dfn_core::{
    over_init, stable, BytesS,
};

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

#[ic_cdk_macros::query]
#[candid_method(query)]
fn transform_http_response(response: HttpResponse) -> HttpResponse {
    xrc::transform_http_response(response)
}

type Timestamp = u64;
type Symbol = String;

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Debug)]
struct CachedExchangeRateInformation {
    information: ExchangeRateInformation,
    time_when_cached: Timestamp,
}

#[derive(Serialize, Deserialize, CandidType, Clone, Eq, PartialEq, Debug)]
struct State {
    /// The cached cryptocurrency rates, indexed by timestamp and symbol.
    cached_rates: BTreeMap<Timestamp, BTreeMap<Symbol, CachedExchangeRateInformation>>,
    // forex_rates: Ring-buffer for forex data.
}

impl State {
    fn default() -> Self {
        Self {
            cached_rates: BTreeMap::new()
        }
    }

    fn encode(&self) -> Vec<u8> {
        encode_one(&self).unwrap()
    }

    fn decode(bytes: &[u8]) -> Result<Self, String> {
        decode_one(bytes)
            .map_err(|err| format!("Decoding exchange rate canister state failed: {}", err))
    }
}

lazy_static! {
    static ref STATE: RwLock<State> = RwLock::new(State::default());
}

#[export_name = "canister_pre_upgrade"]
fn pre_upgrade() {
    let bytes = &STATE
        .read()
        // This should never happen, but it's better to be safe than sorry.
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .encode();
    print(format!(
        "[XRC] serialized state prior to upgrade ({} bytes)",
        bytes.len(),
    ));
    stable::set(bytes);
}

#[export_name = "canister_post_upgrade"]
fn post_upgrade() {
    over_init(|_: BytesS| {
        let bytes = stable::get();
        print(format!(
            "[XRC] deserializing state after upgrade ({} bytes)",
            bytes.len(),
        ));

        *STATE.write().unwrap() = State::decode(&bytes).unwrap();
    })
}

fn main() {}

#[cfg(test)]
mod test {
    use std::path::PathBuf;

    use super::*;

    use ic_cdk::export::candid as cdk_candid;
    use xrc::candid::{AssetClass, ExchangeRateInformationMetadata};

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

    #[test]
    fn test_state_encode() {
        let mut state = State::default();
        let mut cached_rate = BTreeMap::new();
        cached_rate.insert("icp".to_string(), CachedExchangeRateInformation {
            information: ExchangeRateInformation {
                base_asset: Asset { symbol: "icp".to_string(), class: AssetClass::Cryptocurrency },
                quote_asset: Asset { symbol: "usdt".to_string(), class: AssetClass::Cryptocurrency },
                timestamp: 1620296820,
                rate_permyriad: 12345678,
                metadata: ExchangeRateInformationMetadata {
                    number_of_queried_sources: 12,
                    number_of_received_rates: 9,
                    standard_deviation_permyriad: 54321
                }
            },
            time_when_cached: 1620296820,
        });
        state.cached_rates.insert(1620296820, cached_rate);

        let bytes = state.encode();

        let state2 = State::decode(&bytes).unwrap();

        assert_eq!(state, state2);
    }

}
