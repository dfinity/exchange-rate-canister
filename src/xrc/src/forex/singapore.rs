use std::collections::HashMap;

use chrono::NaiveDateTime;
use serde::Deserialize;

use crate::{ExtractError, ONE_KIB, RATE_UNIT};

use super::{ForexRateMap, IsForex, MonetaryAuthorityOfSingapore, ONE_DAY};

#[derive(Deserialize)]
struct MonetaryAuthorityOfSingaporeResponse {
    result: MonetaryAuthorityOfSingaporeResponseResult,
}

#[derive(Deserialize)]
struct MonetaryAuthorityOfSingaporeResponseResult {
    records: Vec<HashMap<String, String>>,
}

impl IsForex for MonetaryAuthorityOfSingapore {
    fn format_timestamp(&self, timestamp: u64) -> String {
        format!(
            "{}",
            NaiveDateTime::from_timestamp(timestamp.try_into().unwrap_or(0), 0).format("%Y-%m-%d")
        )
    }

    fn extract_rate(&self, bytes: &[u8], timestamp: u64) -> Result<ForexRateMap, ExtractError> {
        let response = serde_json::from_slice::<MonetaryAuthorityOfSingaporeResponse>(bytes)
            .map_err(|err| ExtractError::json_deserialize(bytes, err.to_string()))?;
        let timestamp = (timestamp / ONE_DAY) * ONE_DAY;

        let map = response.result.records.get(0).ok_or_else(|| {
            ExtractError::json_deserialize(bytes, "Missing record index".to_string())
        })?;

        let extracted_timestamp = {
            let maybe_end_of_day = map.get("end_of_day");
            let end_of_day = match maybe_end_of_day {
                Some(end_of_day) => NaiveDateTime::parse_from_str(
                    &(end_of_day.to_string() + " 00:00:00"),
                    "%Y-%m-%d %H:%M:%S",
                )
                .unwrap_or_else(|_| NaiveDateTime::from_timestamp(0, 0)),
                None => NaiveDateTime::from_timestamp(0, 0),
            };
            end_of_day.timestamp() as u64
        };

        if extracted_timestamp != timestamp {
            return Err(ExtractError::RateNotFound {
                filter: "Invalid timestamp".to_string(),
            });
        }

        let mut values = map
            .iter()
            .filter_map(|(key, value)| {
                if !key.contains("_sgd") {
                    return None;
                }

                match value.parse::<f64>() {
                    Ok(rate) => match key.split('_').next() {
                        Some(symbol) => {
                            let scaled_rate = if key.ends_with("_100") {
                                (rate * (RATE_UNIT as f64 / 100.0)) as u64
                            } else {
                                (rate * RATE_UNIT as f64) as u64
                            };

                            Some((symbol.to_uppercase(), scaled_rate))
                        }
                        None => None,
                    },
                    Err(_) => None,
                }
            })
            .collect::<ForexRateMap>();
        values.insert("SGD".to_string(), RATE_UNIT);
        self.normalize_to_usd(&values)
    }

    fn get_base_url(&self) -> &str {
        "https://eservices.mas.gov.sg/api/action/datastore/search.json?resource_id=95932927-c8bc-4e7a-b484-68a66a24edfe&limit=100&filters[end_of_day]=DATE"
    }

    fn supports_ipv6(&self) -> bool {
        true
    }

    fn get_utc_offset(&self) -> i16 {
        8
    }

    fn max_response_bytes(&self) -> u64 {
        // 3 KiB
        ONE_KIB * 3
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use crate::{forex::Forex, utils::test::load_file};

    /// The function test if the macro correctly generates the
    /// [core::fmt::Display] trait's implementation for [Forex].
    #[test]
    fn to_string() {
        let forex = Forex::MonetaryAuthorityOfSingapore(MonetaryAuthorityOfSingapore);
        assert_eq!(forex.to_string(), "MonetaryAuthorityOfSingapore");
    }

    /// The function tests if the macro correctly generates derive copies by
    /// verifying that the forex sources return the correct query string.
    #[test]
    fn get_url() {
        let timestamp = 1661524016;
        let singapore = MonetaryAuthorityOfSingapore;
        let query_string = singapore.get_url(timestamp);
        assert_eq!(query_string, "https://eservices.mas.gov.sg/api/action/datastore/search.json?resource_id=95932927-c8bc-4e7a-b484-68a66a24edfe&limit=100&filters[end_of_day]=2022-08-26");
    }

    /// The function tests if the [MonetaryAuthorityOfSingapore] struct returns the correct forex rate.
    #[test]
    fn extract_rate() {
        let singapore = MonetaryAuthorityOfSingapore;
        let query_response = load_file("test-data/forex/monetary-authority-of-singapore.json");
        let timestamp: u64 = 1656374400;
        let extracted_rates = singapore.extract_rate(&query_response, timestamp);

        assert!(matches!(extracted_rates, Ok(ref rates) if rates["EUR"] == 1_058_173_944));
        assert!(matches!(extracted_rates, Ok(ref rates) if rates["JPY"] == 7_390_111));
    }

    /// This function tests that the forex sources can report the max response bytes needed to make a successful HTTP outcall.
    #[test]
    fn max_response_bytes() {
        let forex = Forex::MonetaryAuthorityOfSingapore(MonetaryAuthorityOfSingapore);
        assert_eq!(forex.max_response_bytes(), 3 * ONE_KIB);
    }
}
