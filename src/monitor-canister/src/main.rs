use candid::candid_method;
use ic_cdk_macros::{heartbeat, init, post_upgrade, query};
use monitor_canister::{
    types::{Config, GetEntriesRequest, GetEntriesResponse},
    CanisterEnvironment,
};

#[query]
#[candid_method(query)]
fn get_entries(request: GetEntriesRequest) -> GetEntriesResponse {
    let env = CanisterEnvironment;
    monitor_canister::get_entries(&env, request)
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
    let env = CanisterEnvironment;
    monitor_canister::heartbeat(&env);
}

fn main() {}
