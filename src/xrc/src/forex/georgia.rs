use chrono::NaiveDateTime;
use serde::Deserialize;

use crate::{ExtractError, ONE_DAY_SECONDS, ONE_KIB, RATE_UNIT};

use super::{CentralBankOfGeorgia, ForexRateMap, IsForex};

/// Central Bank of Georgia
#[derive(Debug, Deserialize)]
struct CentralBankOfGeorgiaStruct {
    date: String,
    currencies: Vec<CentralBankOfGeorgiaCurrency>,
}

#[derive(Debug, Deserialize)]
struct CentralBankOfGeorgiaCurrency {
    code: String,
    quantity: u64,
    rate: f64,
}

impl IsForex for CentralBankOfGeorgia {
    fn format_timestamp(&self, timestamp: u64) -> String {
        // TODO(DEFI-2648): Migrate to non-deprecated.
        #[allow(deprecated)]
        NaiveDateTime::from_timestamp_opt(timestamp.try_into().unwrap_or(0), 0)
            .map(|t| t.format("%Y-%m-%d").to_string())
            .unwrap_or_default()
    }

    fn extract_rate(&self, bytes: &[u8], timestamp: u64) -> Result<ForexRateMap, ExtractError> {
        let response = serde_json::from_slice::<Vec<CentralBankOfGeorgiaStruct>>(bytes)
            .map_err(|err| ExtractError::json_deserialize(bytes, err.to_string()))?;

        let timestamp = (timestamp / ONE_DAY_SECONDS) * ONE_DAY_SECONDS;
        let obj = response.first().ok_or(ExtractError::RateNotFound {
            filter: "Cannot find data for timestamp".to_string(),
        })?;
        // TODO(DEFI-2648): Migrate to non-deprecated.
        #[allow(deprecated)]
        let extracted_timestamp = NaiveDateTime::parse_from_str(&obj.date, "%Y-%m-%dT%H:%M:%S%.3fZ")
            .map(|t| t.timestamp())
            .unwrap_or_else(|_| {
                NaiveDateTime::from_timestamp_opt(0, 0)
                    .map(|t| t.timestamp())
                    .unwrap_or_default()
            }) as u64;
        if extracted_timestamp != timestamp {
            return Err(ExtractError::RateNotFound {
                filter: format!(
                    "Incorrect timestamp returned: {} (requested: {})",
                    extracted_timestamp, timestamp
                ),
            });
        }
        let values = obj
            .currencies
            .iter()
            .map(|currency| {
                (
                    currency.code.clone(),
                    ((currency.rate / currency.quantity as f64) * RATE_UNIT as f64) as u64,
                )
            })
            .collect::<ForexRateMap>();
        if values.is_empty() {
            return Err(ExtractError::RateNotFound {
                filter: "Cannot find data for timestamp".to_string(),
            });
        }

        self.normalize_to_usd(&values)
    }

    fn get_base_url(&self) -> &str {
        "https://nbg.gov.ge/gw/api/ct/monetarypolicy/currencies/en/json/?date=DATE"
    }

    fn get_utc_offset(&self) -> i16 {
        4
    }

    fn offset_timestamp_for_query(&self, timestamp: u64) -> u64 {
        // To fetch the rates for day X, Central Bank of Georgia expects the supplied argument to be the day of X+1.
        ((timestamp / ONE_DAY_SECONDS) + 1) * ONE_DAY_SECONDS
    }

    fn max_response_bytes(&self) -> u64 {
        ONE_KIB * 100
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
        let forex = Forex::CentralBankOfGeorgia(CentralBankOfGeorgia);
        assert_eq!(forex.to_string(), "CentralBankOfGeorgia");
    }

    /// The function tests if the macro correctly generates derive copies by
    /// verifying that the forex sources return the correct query string.
    #[test]
    fn query_string() {
        let timestamp = 1661524016;
        let forex = CentralBankOfGeorgia;
        assert_eq!(
            forex.get_url(timestamp),
            "https://nbg.gov.ge/gw/api/ct/monetarypolicy/currencies/en/json/?date=2022-08-26"
        );
    }

    #[test]
    fn max_response_bytes() {
        let forex = Forex::CentralBankOfGeorgia(CentralBankOfGeorgia);
        assert_eq!(forex.max_response_bytes(), 100 * ONE_KIB);
    }

    #[test]
    fn extract_rate() {
        let forex = CentralBankOfGeorgia;
        let query_response = load_file("test-data/forex/central-bank-of-georgia.json");
        let timestamp: u64 = 1656374400;
        let extracted_rates = forex.extract_rate(&query_response, timestamp);
        assert!(matches!(extracted_rates, Ok(ref rates) if rates["EUR"] == 1_058_502_845));
        assert!(matches!(extracted_rates, Ok(ref rates) if rates["JPY"] == 7_395_822));
    }
}
