use candid::Nat;
use ic_cdk::export::candid::decode_one;

use crate::{
    state::with_entries,
    types::{Entry, GetEntriesRequest, GetEntriesResponse},
};

fn decode_entry(idx: usize, bytes: &[u8]) -> Entry {
    decode_one(bytes).unwrap_or_else(|err| {
        ic_cdk::api::trap(&format!("failed to decode entry {}: {}", idx, err))
    })
}

pub fn get_entries(request: GetEntriesRequest) -> GetEntriesResponse {
    let (start, limit) = request
        .offset_and_limit()
        .unwrap_or_else(|err| ic_cdk::api::trap(&err));

    let mut end = start.saturating_add(limit);
    let total = with_entries(|entries| entries.len());
    if end >= total {
        end = total;
    }

    let entries = with_entries(|entries| {
        (start..end)
            .map(|idx| decode_entry(idx, &entries.get(idx).unwrap()))
            .collect()
    });

    GetEntriesResponse {
        entries,
        total: Nat::from(total),
    }
}
