use monitor_canister::{
    types::{Config, GetEntriesRequest, GetEntriesResponse},
    CanisterEnvironment,
};

#[ic_cdk::query]
fn get_entries(request: GetEntriesRequest) -> GetEntriesResponse {
    let env = CanisterEnvironment;
    monitor_canister::get_entries(&env, request)
}

#[ic_cdk::init]
fn init(config: Config) {
    monitor_canister::init(config)
}

#[ic_cdk::post_upgrade]
fn post_upgrade() {}

#[ic_cdk::heartbeat]
fn heartbeat() {
    let env = CanisterEnvironment;
    monitor_canister::heartbeat(&env);
}

fn main() {}
