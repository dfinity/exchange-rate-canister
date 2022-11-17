mod periodic;
mod state;
pub mod types;

use ic_cdk::export::candid::{decode_one, Nat};
use state::{init_config, with_config, with_entries};
use types::{Config, Entry, GetEntriesRequest, GetEntriesResponse};

fn decode_entry(idx: usize, bytes: &[u8]) -> Entry {
    decode_one(bytes).unwrap_or_else(|err| {
        ic_cdk::api::trap(&format!("failed to decode entry {}: {}", idx, err))
    })
}

pub fn init(config: Config) {
    init_config(config)
}

pub fn heartbeat() {
    periodic::beat();
}

pub fn get_entries(request: GetEntriesRequest) -> GetEntriesResponse {
    let (offset, limit) = request
        .offset_and_limit()
        .unwrap_or_else(|err| ic_cdk::api::trap(&err));

    let total = with_entries(|entries| entries.len());

    let entries = with_entries(|entries| {
        (offset..limit)
            .map(|idx| decode_entry(idx, &entries.get(idx).unwrap()))
            .collect()
    });

    GetEntriesResponse {
        entries,
        total: Nat::from(total),
    }
}
