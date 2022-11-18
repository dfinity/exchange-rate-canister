use candid::encode_one;
use std::cell::Cell;
use xrc::candid::{Asset, AssetClass, GetExchangeRateRequest, GetExchangeRateResult};

use crate::state::{with_config, with_entries};

const XRC_REQUEST_CYCLES_COST: u64 = 5_000_000_000;
const NANOS_PER_SEC: u64 = 1_000_000_000;

thread_local! {
    static IS_CALLING_XRC: Cell<bool>  = Cell::new(false);
}

fn is_calling_xrc() -> bool {
    IS_CALLING_XRC.with(|c| c.get())
}

fn set_is_calling_xrc(is_calling: bool) {
    IS_CALLING_XRC.with(|c| c.set(is_calling));
}

pub(crate) fn beat() {
    if is_calling_xrc() {
        return;
    }

    ic_cdk::spawn(call_xrc())
}

async fn call_xrc() {
    set_is_calling_xrc(true);
    let canister_id = with_config(|config| config.xrc_canister_id);
    let now_secs = ((ic_cdk::api::time() / NANOS_PER_SEC) / 60) * 60;
    let one_minute_ago_secs = now_secs - 60;
    let call_result = ic_cdk::api::call::call_with_payment::<_, (GetExchangeRateResult,)>(
        canister_id,
        "get_exchange_rate",
        (GetExchangeRateRequest {
            base_asset: Asset {
                symbol: "ICP".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            quote_asset: Asset {
                symbol: "CXDR".to_string(),
                class: AssetClass::FiatCurrency,
            },
            timestamp: Some(one_minute_ago_secs),
        },),
        XRC_REQUEST_CYCLES_COST,
    )
    .await;

    match call_result {
        Ok(get_exchange_result) => with_entries(|entries| match encode_one(get_exchange_result) {
            Ok(bytes) => match entries.append(&bytes) {
                Ok(_) => {}
                Err(_) => {
                    ic_cdk::println!("No more space to append results")
                }
            },
            Err(_) => {
                ic_cdk::println!("Failed to decode GetExchangeResponse");
            }
        }),
        Err(err) => {
            ic_cdk::println!("{}", err.1);
        }
    }

    set_is_calling_xrc(false);
}
