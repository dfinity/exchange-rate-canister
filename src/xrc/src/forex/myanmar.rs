use std::collections::HashMap;

use chrono::NaiveDateTime;
use serde::Deserialize;

use crate::{ExtractError, ONE_KIB, RATE_UNIT};

use super::{CentralBankOfMyanmar, ForexRateMap, IsForex, SECONDS_PER_DAY};

#[derive(Debug, Deserialize)]
struct CentralBankOfMyanmarResponse {
    timestamp: u64,
    rates: HashMap<String, String>,
}

impl IsForex for CentralBankOfMyanmar {
    fn format_timestamp(&self, timestamp: u64) -> String {
        format!(
            "{}",
            NaiveDateTime::from_timestamp(timestamp.try_into().unwrap_or(0), 0).format("%d-%m-%Y")
        )
    }

    fn extract_rate(&self, bytes: &[u8], timestamp: u64) -> Result<ForexRateMap, ExtractError> {
        let response = serde_json::from_slice::<CentralBankOfMyanmarResponse>(bytes)
            .map_err(|err| ExtractError::json_deserialize(bytes, err.to_string()))?;
        let timestamp = (timestamp / SECONDS_PER_DAY) * SECONDS_PER_DAY;

        if response.timestamp != timestamp {
            return Err(ExtractError::RateNotFound {
                filter: "Invalid timestamp".to_string(),
            });
        }

        let values = response
            .rates
            .iter()
            .filter_map(|(asset, rate)| {
                let parsed = rate.replace(',', "").parse::<f64>().ok()?;
                let mut rate = (parsed * RATE_UNIT as f64) as u64;
                if asset == "JPY" {
                    rate /= 100;
                }
                Some((asset.to_uppercase(), rate))
            })
            .collect::<ForexRateMap>();
        self.normalize_to_usd(&values)
    }

    fn get_base_url(&self) -> &str {
        "https://forex.cbm.gov.mm/api/history/DATE"
    }

    fn supports_ipv6(&self) -> bool {
        true
    }

    fn get_utc_offset(&self) -> i16 {
        // Myanmar timezone is UTC+6.5. To avoid using floating point types here, we use a truncated offset.
        6
    }

    fn max_response_bytes(&self) -> u64 {
        // 3KiB - this is need to get past the http body size limit
        ONE_KIB * 3
    }
}

#[cfg(test)]
mod test {
    use crate::{forex::Forex, utils::test::load_file};

    use super::*;

    /// The function test if the macro correctly generates the
    /// [core::fmt::Display] trait's implementation for [Forex].
    #[test]
    fn to_string() {
        let forex = Forex::CentralBankOfMyanmar(CentralBankOfMyanmar);
        assert_eq!(forex.to_string(), "CentralBankOfMyanmar");
    }

    /// The function tests if the macro correctly generates derive copies by
    /// verifying that the forex sources return the correct query string.
    #[test]
    fn get_url() {
        let timestamp = 1661524016;
        let myanmar = CentralBankOfMyanmar;
        let query_string = myanmar.get_url(timestamp);
        assert_eq!(
            query_string,
            "https://forex.cbm.gov.mm/api/history/26-08-2022"
        );
    }

    /// The function tests if the [CentralBankOfMyanmar] struct returns the correct forex rate.
    #[test]
    fn extract_rate() {
        let myanmar = CentralBankOfMyanmar;
        let query_response = load_file("test-data/forex/central-bank-of-myanmar.json");
        let timestamp: u64 = 1656374400;
        let extracted_rates = myanmar.extract_rate(&query_response, timestamp);
        assert!(matches!(extracted_rates, Ok(ref rates) if rates["EUR"] == 1_059_297_297));
        assert!(matches!(extracted_rates, Ok(ref rates) if rates["JPY"] == 7_369_729));
    }

    /// This function tests that the forex sources can report the max response bytes needed to make a successful HTTP outcall.
    #[test]
    fn max_response_bytes() {
        let forex = Forex::CentralBankOfMyanmar(CentralBankOfMyanmar);
        assert_eq!(forex.max_response_bytes(), 3 * ONE_KIB);
    }
}
