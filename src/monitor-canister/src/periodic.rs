use async_trait::async_trait;
use candid::encode_one;
use ic_cdk::export::Principal;
use std::cell::Cell;
use xrc::candid::{Asset, AssetClass, GetExchangeRateRequest, GetExchangeRateResult};

use crate::{
    state::{with_config, with_entries},
    types::{Entry, EntryError},
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

#[async_trait]
trait Xrc {
    async fn get_exchange_rate(
        &self,
        request: GetExchangeRateRequest,
    ) -> Result<GetExchangeRateResult, EntryError>;
}

struct XrcImpl {
    canister_id: Principal,
}

impl XrcImpl {
    fn new() -> Self {
        Self {
            canister_id: with_config(|config| config.xrc_canister_id),
        }
    }
}

#[async_trait]
impl Xrc for XrcImpl {
    async fn get_exchange_rate(
        &self,
        request: GetExchangeRateRequest,
    ) -> Result<GetExchangeRateResult, EntryError> {
        ic_cdk::api::call::call_with_payment::<_, (GetExchangeRateResult,)>(
            self.canister_id,
            "get_exchange_rate",
            (request.clone(),),
            XRC_REQUEST_CYCLES_COST,
        )
        .await
        .map(|result| result.0)
        .map_err(|(rejection_code, err)| EntryError {
            rejection_code,
            err,
        })
    }
}

pub(crate) fn beat() {
    if is_calling_xrc() {
        return;
    }

    let now_secs = ((ic_cdk::api::time() / NANOS_PER_SEC) / 60) * 60;
    if now_secs < next_call_at_timestamp() {
        return;
    }

    let xrc_impl = XrcImpl::new();
    ic_cdk::spawn(call_xrc(xrc_impl, now_secs))
}

async fn call_xrc(xrc_impl: impl Xrc, now_secs: u64) {
    set_is_calling_xrc(true);

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

    let call_result = xrc_impl.get_exchange_rate(request.clone()).await;
    let mut entry = Entry {
        request,
        result: None,
        error: None,
    };

    match call_result {
        Ok(get_exchange_result) => {
            entry.result = Some(get_exchange_result);
        }
        Err(err) => {
            entry.error = Some(err);
        }
    };

    let bytes = match encode_one(entry) {
        Ok(bytes) => bytes,
        Err(_) => {
            ic_cdk::println!("Failed to encode Entry");
            return;
        }
    };

    with_entries(|entries| {
        if let Err(err) = entries.append(&bytes) {
            ic_cdk::println!("No more space to append results: {:?}", err);
        }
    });

    set_is_calling_xrc(false);
    set_next_call_at_timestamp(now_secs.saturating_add(ONE_MINUTE_SECONDS));
}
