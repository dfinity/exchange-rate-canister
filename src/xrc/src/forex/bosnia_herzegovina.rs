use chrono::NaiveDateTime;
use serde::Deserialize;

use crate::{ExtractError, ONE_KIB, RATE_UNIT};

use super::{CentralBankOfBosniaHerzegovina, ForexRateMap, IsForex, ONE_DAY_SECONDS};

#[derive(Debug, Deserialize)]
struct CentralBankOfBosniaHerzegovinaResponseCurrencyExchangeItem {
    #[serde(rename(deserialize = "AlphaCode"))]
    alpha_code: String,
    #[serde(rename(deserialize = "Units"))]
    units: String,
    #[serde(rename(deserialize = "Middle"))]
    middle: String,
}

#[derive(Debug, Deserialize)]
struct CentralBankOfBosniaHerzegovinaResponse {
    #[serde(rename(deserialize = "CurrencyExchangeItems"))]
    currency_exchange_items: Vec<CentralBankOfBosniaHerzegovinaResponseCurrencyExchangeItem>,
    #[serde(rename(deserialize = "Date"))]
    date: String,
}

/// Central Bank of Bosnia-Herzegovina
impl IsForex for CentralBankOfBosniaHerzegovina {
    fn format_timestamp(&self, timestamp: u64) -> String {
        format!(
            "{}",
            NaiveDateTime::from_timestamp(timestamp.try_into().unwrap_or(0), 0).format("%m-%d-%Y")
        )
    }

    fn extract_rate(&self, bytes: &[u8], timestamp: u64) -> Result<ForexRateMap, ExtractError> {
        let response = serde_json::from_slice::<CentralBankOfBosniaHerzegovinaResponse>(bytes)
            .map_err(|err| ExtractError::json_deserialize(bytes, err.to_string()))?;
        let timestamp = (timestamp / ONE_DAY_SECONDS) * ONE_DAY_SECONDS;

        let extracted_timestamp = NaiveDateTime::parse_from_str(&response.date, "%Y-%m-%dT%H:%M:%S")
            .unwrap_or_else(|_| NaiveDateTime::from_timestamp(0, 0))
            .timestamp() as u64;
        if extracted_timestamp != timestamp {
            return Err(ExtractError::RateNotFound {
                filter: "Invalid timestamp".to_string(),
            });
        }

        let values = response
            .currency_exchange_items
            .iter()
            .filter_map(|item| {
                let units = item.units.parse::<u64>().ok()?;
                let middle = item.middle.replace(',', ".").parse::<f64>().ok()?;
                let rate = ((middle * RATE_UNIT as f64) / units as f64) as u64;

                Some((item.alpha_code.clone(), rate))
            })
            .collect::<ForexRateMap>();
        self.normalize_to_usd(&values)
    }

    fn get_base_url(&self) -> &str {
        "https://www.cbbh.ba/CurrencyExchange/GetJson?date=DATE%2000%3A00%3A00"
    }

    fn supports_ipv6(&self) -> bool {
        true
    }

    fn get_utc_offset(&self) -> i16 {
        1
    }

    fn offset_timestamp_for_query(&self, timestamp: u64) -> u64 {
        // To fetch the rates for day X, Central Bank of Bosnia-Herzgovina expects the supplied argument to be the day of X+1.
        ((timestamp / ONE_DAY_SECONDS) + 1) * ONE_DAY_SECONDS
    }

    /// Responses are between 20-25 KiB. Set to 30 to give some leeway.
    fn max_response_bytes(&self) -> u64 {
        // 30 KiB
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
        let forex = Forex::CentralBankOfBosniaHerzegovina(CentralBankOfBosniaHerzegovina);
        assert_eq!(forex.to_string(), "CentralBankOfBosniaHerzegovina");
    }

    /// The function tests if the macro correctly generates derive copies by
    /// verifying that the forex sources return the correct query string.
    #[test]
    fn get_url() {
        let timestamp = 1661524016;
        let bosnia = CentralBankOfBosniaHerzegovina;
        let query_string = bosnia.get_url(timestamp);
        assert_eq!(
            query_string,
            "https://www.cbbh.ba/CurrencyExchange/GetJson?date=08-26-2022%2000%3A00%3A00"
        );
    }

    /// The function tests if the [CentralBankOfBosniaHerzegovina] struct returns the correct forex rate.
    #[test]
    fn extract_rate() {
        let bosnia = CentralBankOfBosniaHerzegovina;
        let query_response = load_file("test-data/forex/central-bank-of-bosnia-herzegovina.json");
        let timestamp: u64 = 1656374400;
        let extracted_rates = bosnia.extract_rate(&query_response, timestamp);
        assert!(matches!(extracted_rates, Ok(ref rates) if rates["EUR"] == 1_057_200_262));
        assert!(matches!(extracted_rates, Ok(ref rates) if rates["JPY"] == 7_380_104));
    }

    /// This function tests that the forex sources can report the max response bytes needed to make a successful HTTP outcall.
    #[test]
    fn max_response_bytes() {
        let forex = Forex::CentralBankOfBosniaHerzegovina(CentralBankOfBosniaHerzegovina);
        assert_eq!(forex.max_response_bytes(), 30 * ONE_KIB);
    }
}
