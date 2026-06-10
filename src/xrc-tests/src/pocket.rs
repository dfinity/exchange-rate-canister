//! PocketIC test harness for the XRC.
//!
//! Replaces the Docker + dfx + nginx harness for scenarios that have been ported. It installs
//! the XRC and a cycles-forwarding proxy into a PocketIC instance, and answers the XRC's HTTPS
//! outcalls from the same `mock_responses` dataset the old harness used — but via
//! `mock_canister_http_response` instead of an nginx file server. PocketIC applies the XRC's
//! registered transform to each mocked response, exactly as the replica does on mainnet.
//!
//! Two things differ from a naive "call and await" flow:
//!   * The XRC gates `get_exchange_rate` on attached cycles, which ingress messages cannot carry,
//!     so calls are routed through the proxy canister (see `src/xrc-test-proxy`).
//!   * The forex rate store is populated by the canister heartbeat (the old harness slept ~10s for
//!     this), so [`XrcTestEnv::setup`] ticks and answers the heartbeat's forex outcalls before any
//!     `get_exchange_rate` call is made.

use std::collections::HashMap;

use candid::{Decode, Encode, Principal};
use ic_xrc_types::{GetExchangeRateRequest, GetExchangeRateResult};
use pocket_ic::common::rest::{
    CanisterHttpReply, CanisterHttpResponse, MockCanisterHttpResponse,
};
use pocket_ic::{PocketIc, PocketIcBuilder, Time};

use crate::response::{ExchangeResponse, ResponseBody};

/// One trillion cycles.
const TC: u128 = 1_000_000_000_000;

/// Upper bound on rounds spent draining outcalls, so a never-satisfied outcall fails the test
/// instead of hanging.
const MAX_ROUNDS: usize = 512;

/// Number of consecutive rounds with no pending outcall after which the canister is considered
/// done producing outcalls for the current operation.
const IDLE_ROUNDS_TO_SETTLE: usize = 5;

/// A running PocketIC instance with the XRC and the cycles-forwarding proxy installed.
pub struct XrcTestEnv {
    pic: PocketIc,
    proxy: Principal,
    /// Outcall URL -> canned response. The URL is produced by the XRC's own
    /// `exchange.get_url(..)` / `forex.get_url(..)`, the same functions `mock_responses` uses,
    /// so the match is an exact string comparison.
    mocks: HashMap<String, ExchangeResponse>,
}

impl XrcTestEnv {
    /// Brings up the instance, installs both canisters, and lets the heartbeat populate the forex
    /// store. `now_seconds` pins the IC clock; it must match the timestamp the `mock_responses`
    /// forex dataset was built for, so the heartbeat's forex outcall URLs match the mocks.
    pub fn setup(responses: Vec<ExchangeResponse>, now_seconds: u64) -> Self {
        let xrc_wasm = read_wasm("XRC_WASM_PATH");
        let proxy_wasm = read_wasm("PROXY_WASM_PATH");

        let pic = PocketIcBuilder::new().with_application_subnet().build();
        pic.set_time(Time::from_nanos_since_unix_epoch(
            now_seconds.saturating_mul(1_000_000_000),
        ));

        let xrc = pic.create_canister();
        pic.add_cycles(xrc, 100 * TC);
        pic.install_canister(xrc, xrc_wasm, Encode!().unwrap(), None);

        let proxy = pic.create_canister();
        pic.add_cycles(proxy, 100 * TC);
        pic.install_canister(proxy, proxy_wasm, Encode!(&xrc).unwrap(), None);

        let mocks = responses
            .into_iter()
            .map(|r| (r.url.clone(), r))
            .collect();

        let env = Self { pic, proxy, mocks };

        // Let the heartbeat run its catch-up forex fetch and answer those outcalls, so the forex
        // store is populated before any get_exchange_rate call (mirrors the old harness's sleep).
        env.drive_outcalls_until_idle();

        env
    }

    /// Calls `get_exchange_rate` through the proxy (so cycles are attached), answering the XRC's
    /// exchange/stablecoin outcalls as they appear, and returns the decoded result.
    pub fn call_get_exchange_rate(&self, request: &GetExchangeRateRequest) -> GetExchangeRateResult {
        let payload = Encode!(request).unwrap();
        let message_id = self
            .pic
            .submit_call(
                self.proxy,
                Principal::anonymous(),
                "get_exchange_rate",
                payload,
            )
            .expect("failed to submit get_exchange_rate to the proxy");

        self.drive_outcalls_until_idle();

        let bytes = self
            .pic
            .await_call(message_id)
            .expect("get_exchange_rate call was rejected");
        Decode!(&bytes, GetExchangeRateResult).expect("failed to decode GetExchangeRateResult")
    }

    /// Tick the IC, answering every pending HTTPS outcall from the mock dataset, until no outcall
    /// has been pending for [`IDLE_ROUNDS_TO_SETTLE`] consecutive rounds (the XRC fans outcalls out
    /// in waves — exchanges, then stablecoins/forex — so we cannot stop at the first empty round).
    fn drive_outcalls_until_idle(&self) {
        let mut idle = 0;
        for _ in 0..MAX_ROUNDS {
            self.pic.tick();
            let pending = self.pic.get_canister_http();
            if pending.is_empty() {
                idle += 1;
                if idle >= IDLE_ROUNDS_TO_SETTLE {
                    return;
                }
                continue;
            }
            idle = 0;
            for request in pending {
                let response = match self.mocks.get(&request.url) {
                    Some(mock) => reply_for(mock),
                    None => {
                        // Unmatched URL: surface it and answer with a 404 so the source is treated
                        // as failed rather than hanging the loop.
                        eprintln!("no mock response for outcall URL: {}", request.url);
                        CanisterHttpResponse::CanisterHttpReply(CanisterHttpReply {
                            status: 404,
                            headers: vec![],
                            body: vec![],
                        })
                    }
                };
                self.pic.mock_canister_http_response(MockCanisterHttpResponse {
                    subnet_id: request.subnet_id,
                    request_id: request.request_id,
                    response,
                    // Single response shared by all nodes -> deterministic, no consensus divergence.
                    additional_responses: vec![],
                });
            }
        }
        panic!("outcalls did not settle within {MAX_ROUNDS} rounds");
    }
}

/// Build a PocketIC reply from a canned [`ExchangeResponse`]. The raw body is returned unchanged;
/// PocketIC runs the XRC's transform on it (header stripping, rate extraction) as on mainnet.
fn reply_for(mock: &ExchangeResponse) -> CanisterHttpResponse {
    let body = match &mock.body {
        ResponseBody::Json(bytes) | ResponseBody::Xml(bytes) => bytes.clone(),
        ResponseBody::Empty => vec![],
    };
    CanisterHttpResponse::CanisterHttpReply(CanisterHttpReply {
        status: mock.status_code,
        headers: vec![],
        body,
    })
}

/// Read a wasm module from the path in the given environment variable.
fn read_wasm(env_var: &str) -> Vec<u8> {
    let path = std::env::var(env_var)
        .unwrap_or_else(|_| panic!("environment variable {env_var} must point to a wasm module"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("failed to read wasm at {path}: {e}"))
}
