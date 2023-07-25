use candid::{decode_one, Nat};

use crate::{
    environment::Environment,
    state::with_entries,
    types::{Entry, GetEntriesRequest, GetEntriesResponse},
};

fn decode_entry(env: &impl Environment, idx: usize, bytes: &[u8]) -> Entry {
    decode_one(bytes).unwrap_or_else(|err| {
        env.trap(&format!("failed to decode entry {}: {}", idx, err));
    })
}

pub fn get_entries(env: &impl Environment, request: GetEntriesRequest) -> GetEntriesResponse {
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
    use candid::encode_one;

    use super::*;
    use crate::{
        environment::test::TestEnvironment,
        types::{EntryResult, GetEntriesRequest},
    };

    fn fill_entries(amount: u8) {
        let base_asset = ic_xrc_types::Asset {
            symbol: "ICP".to_string(),
            class: ic_xrc_types::AssetClass::Cryptocurrency,
        };
        let quote_asset = ic_xrc_types::Asset {
            symbol: "CXDR".to_string(),
            class: ic_xrc_types::AssetClass::FiatCurrency,
        };
        let timestamp = 1_669_755_360;
        with_entries(|entries| {
            for _ in 0..amount {
                let entry = Entry {
                    request: ic_xrc_types::GetExchangeRateRequest {
                        base_asset: base_asset.clone(),
                        quote_asset: quote_asset.clone(),
                        timestamp: Some(timestamp),
                    },
                    result: EntryResult::Rate(ic_xrc_types::ExchangeRate {
                        base_asset: base_asset.clone(),
                        quote_asset: quote_asset.clone(),
                        timestamp,
                        rate: 2_972_532_915,
                        metadata: ic_xrc_types::ExchangeRateMetadata {
                            decimals: 9,
                            base_asset_num_queried_sources: 1,
                            base_asset_num_received_rates: 1,
                            quote_asset_num_queried_sources: 1,
                            quote_asset_num_received_rates: 1,
                            standard_deviation: 1,
                            forex_timestamp: Some(1_669_755_360),
                        },
                    }),
                };
                entries
                    .append(&encode_one(entry).expect("failed to encode entry"))
                    .expect("failed to append entry's bytes");
            }
        });
    }

    #[test]
    fn get_entries_success() {
        fill_entries(10);

        let env = TestEnvironment::builder().build();
        let response = get_entries(
            &env,
            GetEntriesRequest {
                offset: Nat::from(0),
                limit: None,
            },
        );

        assert_eq!(response.total, 10);
        assert_eq!(response.entries.len(), 10);
    }

    #[test]
    fn get_entries_with_out_of_bounds_limit() {
        let env = TestEnvironment::builder().build();
        let response = get_entries(
            &env,
            GetEntriesRequest {
                offset: Nat::from(0),
                limit: Some(Nat::from(200)),
            },
        );

        assert_eq!(response.total, 0);
        assert!(response.entries.is_empty());
    }

    #[test]
    fn get_entries_with_out_of_bounds_offset() {
        let env = TestEnvironment::builder().build();
        let response = get_entries(
            &env,
            GetEntriesRequest {
                offset: Nat::from(10),
                limit: None,
            },
        );

        assert_eq!(response.total, 0);
        assert!(response.entries.is_empty());
    }
}
