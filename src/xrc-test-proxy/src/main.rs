//! Test-only proxy canister.
//!
//! The XRC's `get_exchange_rate` rejects any call that does not carry at least
//! `XRC_REQUEST_CYCLES_COST` cycles. Ingress messages cannot carry cycles, so PocketIC
//! tests route their calls through this canister, which forwards the request to the XRC
//! with the required cycles attached (the same role the cycles wallet plays for
//! `dfx canister call --wallet --with-cycles`).

use candid::Principal;
use ic_cdk::call::Call;
use ic_xrc_types::{GetExchangeRateRequest, GetExchangeRateResult};
use std::cell::RefCell;

fn main() {}

/// Mirrors `xrc::XRC_REQUEST_CYCLES_COST` (1B). The XRC accepts up to the computed fee and
/// refunds the remainder, so attaching the minimum is always sufficient.
const XRC_REQUEST_CYCLES_COST: u128 = 1_000_000_000;

thread_local! {
    static XRC: RefCell<Option<Principal>> = const { RefCell::new(None) };
}

#[ic_cdk::init]
fn init(xrc: Principal) {
    XRC.with(|cell| cell.replace(Some(xrc)));
}

#[ic_cdk::update]
async fn get_exchange_rate(request: GetExchangeRateRequest) -> GetExchangeRateResult {
    let xrc = XRC.with(|cell| cell.borrow().expect("XRC canister id has not been set"));
    let response = Call::unbounded_wait(xrc, "get_exchange_rate")
        .with_arg(request)
        .with_cycles(XRC_REQUEST_CYCLES_COST)
        .await
        .expect("call to the XRC canister failed");
    response
        .candid::<GetExchangeRateResult>()
        .expect("failed to decode GetExchangeRateResult from the XRC canister")
}
