use chrono::NaiveDateTime;
use serde::Deserialize;

use crate::{ExtractError, ONE_KIB, RATE_UNIT};

use super::{CentralBankOfNepal, ForexRateMap, IsForex, SECONDS_PER_DAY};

/// Central Bank of Nepal
#[derive(Debug, Deserialize)]
struct CentralBankOfNepalStruct {
    status: CentralBankOfNepalStatus,
    #[serde(skip_deserializing)]
    #[serde(rename(deserialize = "errors"))]
    _errors: String,
    #[serde(skip_deserializing)]
    #[serde(rename(deserialize = "params"))]
    _params: String,
    data: CentralBankOfNepalData,
    #[serde(skip_deserializing)]
    #[serde(rename(deserialize = "pagination"))]
    _pagination: String,
}

#[derive(Debug, Deserialize)]
struct CentralBankOfNepalStatus {
    code: u64,
}

#[derive(Debug, Deserialize)]
struct CentralBankOfNepalData {
    payload: Vec<CentralBankOfNepalDataOneDay>,
}

#[derive(Debug, Deserialize)]
struct CentralBankOfNepalDataOneDay {
    date: String,
    #[serde(rename(deserialize = "published_on"))]
    _published_on: String,
    #[serde(rename(deserialize = "modified_on"))]
    _modified_on: String,
    rates: Vec<CentralBankOfNepalDataRate>,
}

#[derive(Debug, Deserialize)]
struct CentralBankOfNepalDataRate {
    currency: CentralBankOfNepalCurrency,
    buy: String,
    sell: String,
}

#[derive(Debug, Deserialize)]
struct CentralBankOfNepalCurrency {
    iso3: String,
    #[serde(rename(deserialize = "name"))]
    _name: String,
    unit: u64,
}

impl IsForex for CentralBankOfNepal {
    fn format_timestamp(&self, timestamp: u64) -> String {
        format!(
            "{}",
            NaiveDateTime::from_timestamp(timestamp.try_into().unwrap_or(0), 0).format("%Y-%m-%d")
        )
    }

    fn extract_rate(&self, bytes: &[u8], timestamp: u64) -> Result<ForexRateMap, ExtractError> {
        let response = serde_json::from_slice::<CentralBankOfNepalStruct>(bytes)
            .map_err(|err| ExtractError::json_deserialize(bytes, err.to_string()))?;

        if response.status.code != 200 {
            return Err(ExtractError::RateNotFound {
                filter: format!(
                    "Cannot find data for timestamp (status code: {})",
                    response.status.code
                ),
            });
        }

        let timestamp = (timestamp / SECONDS_PER_DAY) * SECONDS_PER_DAY;
        let mut values = ForexRateMap::new();

        for day in response.data.payload {
            let extracted_timestamp =
                NaiveDateTime::parse_from_str(&(day.date + " 00:00:00"), "%Y-%m-%d %H:%M:%S")
                    .unwrap_or_else(|_| NaiveDateTime::from_timestamp(0, 0))
                    .timestamp() as u64;
            if extracted_timestamp == timestamp {
                for rate in day.rates {
                    let buy = rate.buy.parse::<f64>();
                    let sell = rate.sell.parse::<f64>();
                    let value = match (buy, sell) {
                        (Ok(buy), Ok(sell)) => ((buy + sell) / 2.0 * (RATE_UNIT as f64)) as u64,
                        _ => continue,
                    };
                    values.insert(rate.currency.iso3, value / rate.currency.unit);
                }
                break;
            }
        }
        if values.is_empty() {
            return Err(ExtractError::RateNotFound {
                filter: "Cannot find data for timestamp".to_string(),
            });
        }

        self.normalize_to_usd(&values)
    }

    fn get_base_url(&self) -> &str {
        "https://www.nrb.org.np/api/forex/v1/rates?from=DATE&to=DATE&per_page=100&page=1"
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
    use super::*;

    use crate::{forex::Forex, utils::test::load_file};

    /// The function test if the macro correctly generates the
    /// [core::fmt::Display] trait's implementation for [Forex].
    #[test]
    fn to_string() {
        let forex = Forex::CentralBankOfNepal(CentralBankOfNepal);
        assert_eq!(forex.to_string(), "CentralBankOfNepal");
    }

    /// The function tests if the macro correctly generates derive copies by
    /// verifying that the forex sources return the correct query string.
    #[test]
    fn query_string() {
        let timestamp = 1661524016;
        let forex = CentralBankOfNepal;
        assert_eq!(
            forex.get_url(timestamp),
            "https://www.nrb.org.np/api/forex/v1/rates?from=2022-08-26&to=2022-08-26&per_page=100&page=1"
        );
    }

    #[test]
    fn max_response_bytes() {
        let forex = Forex::CentralBankOfNepal(CentralBankOfNepal);
        assert_eq!(forex.max_response_bytes(), 30 * ONE_KIB);
    }

    #[test]
    fn extract_rate() {
        let forex = CentralBankOfNepal;
        let query_response = load_file("test-data/forex/central-bank-of-nepal.json");
        let timestamp: u64 = 1656374400;
        let extracted_rates = forex.extract_rate(&query_response, timestamp);
        assert!(matches!(extracted_rates, Ok(ref rates) if rates["EUR"] == 1_058_516_154));
        assert!(matches!(extracted_rates, Ok(ref rates) if rates["JPY"] == 7_395_293));
    }
}
