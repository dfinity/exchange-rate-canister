use std::collections::HashMap;

use chrono::NaiveDateTime;
use serde::Deserialize;

use crate::{ExtractError, ONE_KIB, RATE_UNIT};

use super::{BankOfCanada, ForexRateMap, IsForex, ONE_DAY};

#[derive(Debug, Deserialize)]
struct BankOfCanadaResponseSeriesDetail {
    label: String,
}

#[derive(Debug, Deserialize)]
struct BankOfCanadaResponseObservation {
    d: String,
    #[serde(flatten)]
    rates: HashMap<String, BankOfCanadaResponseObservationValue>,
}

#[derive(Debug, Deserialize)]
struct BankOfCanadaResponseObservationValue {
    v: String,
}

#[derive(Debug, Deserialize)]
struct BankOfCanadaResponse {
    #[serde(rename(deserialize = "seriesDetail"))]
    series_detail: HashMap<String, BankOfCanadaResponseSeriesDetail>,
    observations: Vec<BankOfCanadaResponseObservation>,
}

impl IsForex for BankOfCanada {
    fn format_timestamp(&self, timestamp: u64) -> String {
        format!(
            "{}",
            NaiveDateTime::from_timestamp(timestamp.try_into().unwrap_or(0), 0).format("%Y-%m-%d")
        )
    }

    fn extract_rate(&self, bytes: &[u8], timestamp: u64) -> Result<ForexRateMap, ExtractError> {
        let response = serde_json::from_slice::<BankOfCanadaResponse>(bytes)
            .map_err(|err| ExtractError::json_deserialize(bytes, err.to_string()))?;

        let timestamp = (timestamp / ONE_DAY) * ONE_DAY;
        let mut extracted_timestamp: u64;
        let mut values = ForexRateMap::new();
        for observation in response.observations.iter() {
            extracted_timestamp = NaiveDateTime::parse_from_str(
                &(observation.d.to_string() + " 00:00:00"),
                "%Y-%m-%d %H:%M:%S",
            )
            .unwrap_or_else(|_| NaiveDateTime::from_timestamp(0, 0))
            .timestamp() as u64;
            if extracted_timestamp != timestamp {
                return Err(ExtractError::RateNotFound {
                    filter: "Invalid timestamp".to_string(),
                });
            }

            observation.rates.iter().for_each(|(key, value)| {
                let detail = match response.series_detail.get(key) {
                    Some(detail) => detail,
                    None => return,
                };

                let symbol = match detail.label.split('/').next() {
                    Some(symbol) => symbol,
                    None => return,
                };

                let value = match value.v.parse::<f64>() {
                    Ok(value) => value,
                    Err(_) => return,
                };

                values.insert(symbol.to_uppercase(), (value * RATE_UNIT as f64) as u64);
            });
        }

        values.insert("CAD".to_string(), RATE_UNIT);
        self.normalize_to_usd(&values)
    }

    fn get_base_url(&self) -> &str {
        "https://www.bankofcanada.ca/valet/observations/group/FX_RATES_DAILY/json?start_date=DATE&end_date=DATE"
    }

    fn get_utc_offset(&self) -> i16 {
        // Using the westmost timezone (PST)
        -8
    }

    fn max_response_bytes(&self) -> u64 {
        ONE_KIB * 10
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
        let forex = Forex::BankOfCanada(BankOfCanada);
        assert_eq!(forex.to_string(), "BankOfCanada");
    }

    /// The function tests if the macro correctly generates derive copies by
    /// verifying that the forex sources return the correct query string.
    #[test]
    fn get_url() {
        let timestamp = 1661524016;
        let canada = BankOfCanada;
        let query_string = canada.get_url(timestamp);
        assert_eq!(
            query_string,
            "https://www.bankofcanada.ca/valet/observations/group/FX_RATES_DAILY/json?start_date=2022-08-26&end_date=2022-08-26"
        );
    }

    /// The function tests if the [BankOfCanada] struct returns the correct forex rate.
    #[test]
    fn extract_rate() {
        let canada = BankOfCanada;
        let query_response = load_file("test-data/forex/bank-of-canada.json");
        let timestamp: u64 = 1656374400;
        let extracted_rates = canada.extract_rate(&query_response, timestamp);

        assert!(matches!(extracted_rates, Ok(rates) if rates["EUR"] == 1_052_938_432));
    }

    /// This function tests that the forex sources can report the max response bytes needed to make a successful HTTP outcall.
    #[test]
    fn max_response_bytes() {
        let forex = Forex::BankOfCanada(BankOfCanada);
        assert_eq!(forex.max_response_bytes(), 10 * ONE_KIB);
    }
}
