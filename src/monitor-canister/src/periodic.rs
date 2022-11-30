use candid::encode_one;
use std::cell::Cell;
use xrc::candid::{Asset, AssetClass, GetExchangeRateRequest, GetExchangeRateResult};

use crate::{
    state::{with_config, with_entries},
    types::Entry,
};

const ONE_MINUTE_SECONDS: u64 = 60;
const XRC_REQUEST_CYCLES_COST: u64 = 5_000_000_000;
const NANOS_PER_SEC: u64 = 1_000_000_000;

thread_local! {
    static NEXT_CALL_AT_TIMESTAMP: Cell<u64> = Cell::new(0);
    static IS_CALLING_XRC: Cell<bool>  = Cell::new(false);
}

fn is_calling_xrc() -> bool {
    IS_CALLING_XRC.with(|c| c.get())
}

fn set_is_calling_xrc(is_calling: bool) {
    IS_CALLING_XRC.with(|c| c.set(is_calling));
}

fn next_call_at_timestamp() -> u64 {
    NEXT_CALL_AT_TIMESTAMP.with(|c| c.get())
}

fn set_next_call_at_timestamp(timestamp: u64) {
    NEXT_CALL_AT_TIMESTAMP.with(|c| c.set(timestamp))
}

pub(crate) fn beat() {
    if is_calling_xrc() {
        return;
    }

    let now_secs = ((ic_cdk::api::time() / NANOS_PER_SEC) / 60) * 60;
    if now_secs < next_call_at_timestamp() {
        return;
    }

    ic_cdk::spawn(call_xrc(now_secs))
}

async fn call_xrc(now_secs: u64) {
    set_is_calling_xrc(true);
    let canister_id = with_config(|config| config.xrc_canister_id);

    // Request the rate from one minute ago (this is done to ensure we do actually receive some rates).
    let one_minute_ago_secs = now_secs - ONE_MINUTE_SECONDS;
    let request = GetExchangeRateRequest {
        base_asset: Asset {
            symbol: "ICP".to_string(),
            class: AssetClass::Cryptocurrency,
        },
        quote_asset: Asset {
            symbol: "CXDR".to_string(),
            class: AssetClass::FiatCurrency,
        },
        timestamp: Some(one_minute_ago_secs),
    };

    let call_result = ic_cdk::api::call::call_with_payment::<_, (GetExchangeRateResult,)>(
        canister_id,
        "get_exchange_rate",
        (request.clone(),),
        XRC_REQUEST_CYCLES_COST,
    )
    .await;

    match call_result {
        Ok(get_exchange_result) => {
            let entry = Entry {
                request,
                result: get_exchange_result.0,
            };

            with_entries(|entries| match encode_one(entry) {
                Ok(bytes) => match entries.append(&bytes) {
                    Ok(_) => {}
                    Err(_) => {
                        ic_cdk::println!("No more space to append results")
                    }
                },
                Err(_) => {
                    ic_cdk::println!("Failed to decode GetExchangeResponse");
                }
            })
        }
        Err(err) => {
            ic_cdk::println!("{}", err.1);
        }
    }

    set_is_calling_xrc(false);
    set_next_call_at_timestamp(now_secs.saturating_add(ONE_MINUTE_SECONDS));
}
