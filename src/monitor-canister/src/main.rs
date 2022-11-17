use ic_cdk_macros::{heartbeat, query};
use monitor_canister::types::{GetEntriesRequest, GetEntriesResponse};

#[query]
fn get_entries(request: GetEntriesRequest) -> GetEntriesResponse {
    monitor_canister::get_entries(request)
}

#[heartbeat]
fn heartbeat() {
    monitor_canister::heartbeat();
}

fn main() {}
