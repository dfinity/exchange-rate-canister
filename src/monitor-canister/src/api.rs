use candid::Nat;
use ic_cdk::export::candid::decode_one;

use crate::{
    environment::{CanisterEnvironment, Environment},
    state::with_entries,
    types::{Entry, GetEntriesRequest, GetEntriesResponse},
};

fn decode_entry(env: &impl Environment, idx: usize, bytes: &[u8]) -> Entry {
    decode_one(bytes).unwrap_or_else(|err| {
        env.trap(&format!("failed to decode entry {}: {}", idx, err));
    })
}

pub fn get_entries(request: GetEntriesRequest) -> GetEntriesResponse {
    let env = CanisterEnvironment;
    get_entries_internal(&env, request)
}

fn get_entries_internal(env: &impl Environment, request: GetEntriesRequest) -> GetEntriesResponse {
    let (start, limit) = match request.offset_and_limit() {
        Ok(start_and_limit) => start_and_limit,
        Err(err) => {
            env.trap(&err);
        }
    };

    let mut end = start.saturating_add(limit);
    let total = with_entries(|entries| entries.len());
    if end >= total {
        end = total;
    }

    let entries = with_entries(|entries| {
        (start..end)
            .map(|idx| decode_entry(env, idx, &entries.get(idx).unwrap()))
            .collect()
    });

    GetEntriesResponse {
        entries,
        total: Nat::from(total),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{environment::test::TestEnvironment, types::GetEntriesRequest};

    #[test]
    fn get_entries_with_out_of_bounds_limit() {
        let env = TestEnvironment::builder().build();
        let response = get_entries_internal(
            &env,
            GetEntriesRequest {
                offset: Nat::from(0),
                limit: Some(Nat::from(200)),
            },
        );

        assert_eq!(response.total, 0);
        assert!(response.entries.is_empty());
    }
}
