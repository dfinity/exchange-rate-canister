use ic_cdk_macros::{heartbeat, query};

#[query]
fn get_entries() {
    monitor_canister::get_entries()
}

#[heartbeat]
fn heartbeat() {
    monitor_canister::heartbeat();
}

fn main() {}
