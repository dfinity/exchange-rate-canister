use ic_cdk::export::candid::candid_method;
use ic_cdk_macros::{heartbeat, init, post_upgrade, query};
use monitor_canister::types::{Config, GetEntriesRequest, GetEntriesResponse};

#[query]
#[candid_method(query)]
fn get_entries(request: GetEntriesRequest) -> GetEntriesResponse {
    monitor_canister::get_entries(request)
}

#[init]
#[candid_method(init)]
fn init(config: Config) {
    monitor_canister::init(config)
}

#[post_upgrade]
fn post_upgrade() {}

#[heartbeat]
fn heartbeat() {
    monitor_canister::heartbeat();
}

fn main() {}
