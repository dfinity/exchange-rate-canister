use chrono::NaiveDateTime;
use serde::Deserialize;

use crate::{ExtractError, ONE_KIB, RATE_UNIT};

use super::{BankOfItaly, ForexRateMap, IsForex, SECONDS_PER_DAY};

/// Bank of Italy
#[derive(Debug, Deserialize)]
struct BankOfItalyStruct {
    rates: Vec<BankOfItalyRate>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BankOfItalyRate {
    iso_code: String,
    avg_rate: String,
    exchange_convention_code: String,
    reference_date: String,
}

impl IsForex for BankOfItaly {
    fn format_timestamp(&self, timestamp: u64) -> String {
        format!(
            "{}",
            NaiveDateTime::from_timestamp(timestamp.try_into().unwrap_or(0), 0).format("%Y-%m-%d")
        )
    }

    fn extract_rate(&self, bytes: &[u8], timestamp: u64) -> Result<ForexRateMap, ExtractError> {
        let response = serde_json::from_slice::<BankOfItalyStruct>(bytes)
            .map_err(|err| ExtractError::json_deserialize(bytes, err.to_string()))?;

        let timestamp = (timestamp / SECONDS_PER_DAY) * SECONDS_PER_DAY;

        let mut values: ForexRateMap = response
            .rates
            .iter()
            .filter_map(|rate| {
                let extracted_timestamp = NaiveDateTime::parse_from_str(
                    &format!("{} 00:00:00", rate.reference_date),
                    "%Y-%m-%d %H:%M:%S",
                )
                .unwrap_or_else(|_| NaiveDateTime::from_timestamp(0, 0))
                .timestamp() as u64;
                if extracted_timestamp == timestamp {
                    rate.avg_rate
                        .parse::<f64>()
                        .map_err(|e| e.to_string())
                        .and_then(|avg_rate| {
                            if avg_rate <= 0.0 {
                                return Err("Rate cannot be below or equal to 0".to_string());
                            }
                            let calculated_rate = if rate.exchange_convention_code == "C" {
                                1.0 / avg_rate
                            } else {
                                avg_rate
                            };

                            Ok((
                                rate.iso_code.clone(),
                                (calculated_rate * RATE_UNIT as f64) as u64,
                            ))
                        })
                        .ok()
                } else {
                    None
                }
            })
            .collect();

        if values.is_empty() {
            return Err(ExtractError::RateNotFound {
                filter: "Cannot find data for timestamp".to_string(),
            });
        }

        values.insert("EUR".to_string(), RATE_UNIT);

        self.normalize_to_usd(&values)
    }

    fn get_additional_http_request_headers(&self) -> Vec<(String, String)> {
        vec![("Accept".to_string(), "application/json".to_string())]
    }

    fn get_base_url(&self) -> &str {
        "https://tassidicambio.bancaditalia.it/terzevalute-wf-web/rest/v1.0/dailyRates?referenceDate=DATE&currencyIsoCode=EUR"
    }

    fn get_utc_offset(&self) -> i16 {
        1
    }

    fn max_response_bytes(&self) -> u64 {
        ONE_KIB * 30
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
        let forex = Forex::BankOfItaly(BankOfItaly);
        assert_eq!(forex.to_string(), "BankOfItaly");
    }

    /// The function tests if the macro correctly generates derive copies by
    /// verifying that the forex sources return the correct query string.
    #[test]
    fn query_string() {
        let timestamp = 1661524016;
        let forex = BankOfItaly;
        assert_eq!(
            forex.get_url(timestamp),
            "https://tassidicambio.bancaditalia.it/terzevalute-wf-web/rest/v1.0/dailyRates?referenceDate=2022-08-26&currencyIsoCode=EUR"
        );
    }

    #[test]
    fn max_response_bytes() {
        let forex = Forex::BankOfItaly(BankOfItaly);
        assert_eq!(forex.max_response_bytes(), 30 * ONE_KIB);
    }

    #[test]
    fn extract_rate() {
        let forex = BankOfItaly;
        let query_response = load_file("test-data/forex/bank-of-italy.json");
        let timestamp: u64 = 1656374400;
        let extracted_rates = forex.extract_rate(&query_response, timestamp);
        assert!(matches!(extracted_rates, Ok(ref rates) if rates["EUR"] == 1_056_100_000));
        assert!(matches!(extracted_rates, Ok(ref rates) if rates["JPY"] == 7_350_873));
        // The BWP rate in the test JSON file has been modified to use exchangeConventionCode="I", which means the rate is inverted.
        // For this reason, the asserted rate here is fake, but it is used to make sure that we handle this case correctly.
        assert!(matches!(extracted_rates, Ok(ref rates) if rates["BWP"] == 13_601_828_734));
    }
}
