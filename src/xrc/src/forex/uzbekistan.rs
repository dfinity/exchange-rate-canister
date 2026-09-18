use chrono::NaiveDateTime;
use serde::Deserialize;

use crate::utils::checked_scaled_rate;
use crate::{ExtractError, ONE_KIB, RATE_UNIT};

use super::{CentralBankOfUzbekistan, ForexRateMap, IsForex, ONE_DAY_SECONDS};

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
        // TODO(DEFI-2648): Migrate to non-deprecated.
        #[allow(deprecated)]
        NaiveDateTime::from_timestamp_opt(timestamp.try_into().unwrap_or(0), 0)
            .map(|t| t.format("%Y-%m-%d").to_string())
            .unwrap_or_default()
    }

    fn extract_rate(&self, bytes: &[u8], timestamp: u64) -> Result<ForexRateMap, ExtractError> {
        let response = serde_json::from_slice::<Vec<CentralBankOfUzbekistanDetail>>(bytes)
            .map_err(|err| ExtractError::json_deserialize(bytes, err.to_string()))?;

        let timestamp = (timestamp / ONE_DAY_SECONDS) * ONE_DAY_SECONDS;
        let mut values = ForexRateMap::new();

        for detail in response {
            // TODO(DEFI-2648): Migrate to non-deprecated.
            #[allow(deprecated)]
            let extracted_timestamp =
                NaiveDateTime::parse_from_str(&(detail.date + " 00:00:00"), "%d.%m.%Y %H:%M:%S")
                    .map(|t| t.timestamp())
                    .unwrap_or_else(|_| {
                        NaiveDateTime::from_timestamp_opt(0, 0)
                            .map(|t| t.timestamp())
                            .unwrap_or_default()
                    }) as u64;
            if extracted_timestamp != timestamp {
                return Err(ExtractError::RateNotFound {
                    filter: "Invalid timestamp".to_string(),
                });
            }

            let rate = match detail
                .rate
                .parse::<f64>()
                .ok()
                .and_then(|rate| checked_scaled_rate(rate * RATE_UNIT as f64))
            {
                Some(rate) => rate,
                None => continue,
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

    /// A rate that parses to infinity used to saturate to [u64::MAX] on cast and was then
    /// treated as a genuine quote by the aggregation code.
    #[test]
    fn extract_rate_discards_rates_that_cannot_be_represented() {
        let uzbekistan = CentralBankOfUzbekistan;
        let query_response = br#"[
            {"Ccy":"USD","Rate":"1","Date":"28.06.2022"},
            {"Ccy":"EUR","Rate":"1e309","Date":"28.06.2022"},
            {"Ccy":"GBP","Rate":"-1","Date":"28.06.2022"},
            {"Ccy":"CNY","Rate":"0.15","Date":"28.06.2022"}
        ]"#;
        let timestamp: u64 = 1656374400;

        let rates = uzbekistan
            .extract_rate(query_response, timestamp)
            .expect("should extract the remaining rates");

        assert!(!rates.contains_key("EUR"));
        assert!(!rates.contains_key("GBP"));
        assert_eq!(rates["CNY"], 150_000_000);
    }

    /// This function tests that the forex sources can report the max response bytes needed to make a successful HTTP outcall.
    #[test]
    fn max_response_bytes() {
        let forex = Forex::CentralBankOfUzbekistan(CentralBankOfUzbekistan);
        assert_eq!(forex.max_response_bytes(), 30 * ONE_KIB);
    }
}
