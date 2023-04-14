use chrono::NaiveDateTime;
use serde::Deserialize;

use crate::{ExtractError, ONE_KIB, RATE_UNIT};

use super::{CentralBankOfUzbekistan, ForexRateMap, IsForex, ONE_DAY};

#[derive(Debug, Deserialize)]
struct CentralBankOfUzbekistanDetail {
    #[serde(rename(deserialize = "Ccy"))]
    currency: String,
    #[serde(rename(deserialize = "Rate"))]
    rate: String,
    #[serde(rename(deserialize = "Date"))]
    date: String,
}

impl IsForex for CentralBankOfUzbekistan {
    fn format_timestamp(&self, timestamp: u64) -> String {
        format!(
            "{}",
            NaiveDateTime::from_timestamp(timestamp.try_into().unwrap_or(0), 0).format("%Y-%m-%d")
        )
    }

    fn extract_rate(&self, bytes: &[u8], timestamp: u64) -> Result<ForexRateMap, ExtractError> {
        let response = serde_json::from_slice::<Vec<CentralBankOfUzbekistanDetail>>(bytes)
            .map_err(|err| ExtractError::json_deserialize(bytes, err.to_string()))?;

        let timestamp = (timestamp / ONE_DAY) * ONE_DAY;
        let mut values = ForexRateMap::new();

        for detail in response {
            let extracted_timestamp =
                NaiveDateTime::parse_from_str(&(detail.date + " 00:00:00"), "%d.%m.%Y %H:%M:%S")
                    .unwrap_or_else(|_| NaiveDateTime::from_timestamp(0, 0))
                    .timestamp() as u64;
            if extracted_timestamp != timestamp {
                return Err(ExtractError::RateNotFound {
                    filter: "Invalid timestamp".to_string(),
                });
            }

            let rate = match detail.rate.parse::<f64>() {
                Ok(rate) => (rate * RATE_UNIT as f64) as u64,
                Err(_) => continue,
            };

            values.insert(detail.currency, rate);
        }

        self.normalize_to_usd(&values)
    }

    fn get_base_url(&self) -> &str {
        "https://cbu.uz/ru/arkhiv-kursov-valyut/json/all/DATE/"
    }

    fn get_utc_offset(&self) -> i16 {
        5
    }

    fn max_response_bytes(&self) -> u64 {
        ONE_KIB * 30
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
        let forex = Forex::CentralBankOfUzbekistan(CentralBankOfUzbekistan);
        assert_eq!(forex.to_string(), "CentralBankOfUzbekistan");
    }

    /// The function tests if the macro correctly generates derive copies by
    /// verifying that the forex sources return the correct query string.
    #[test]
    fn get_url() {
        let timestamp = 1661524016;
        let uzbekistan = CentralBankOfUzbekistan;
        let query_string = uzbekistan.get_url(timestamp);
        assert_eq!(
            query_string,
            "https://cbu.uz/ru/arkhiv-kursov-valyut/json/all/2022-08-26/"
        );
    }

    /// The function tests if the [CentralBankOfUzbekistan] struct returns the correct forex rate.
    #[test]
    fn extract_rate() {
        let uzbekistan = CentralBankOfUzbekistan;
        let query_response = load_file("test-data/forex/central-bank-of-uzbekistan.json");
        let timestamp: u64 = 1656374400;
        let extracted_rates = uzbekistan.extract_rate(&query_response, timestamp);

        assert!(matches!(extracted_rates, Ok(rates) if rates["EUR"] == 1_056_900_158));
    }

    /// This function tests that the forex sources can report the max response bytes needed to make a successful HTTP outcall.
    #[test]
    fn max_response_bytes() {
        let forex = Forex::CentralBankOfUzbekistan(CentralBankOfUzbekistan);
        assert_eq!(forex.max_response_bytes(), 30 * ONE_KIB);
    }
}
