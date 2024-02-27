use chrono::NaiveDateTime;
use serde::Deserialize;

use crate::{ExtractError, ONE_KIB, RATE_UNIT};
use super::{IsForex, CentralBankOfTurkey};

#[derive(Deserialize, Debug)]
struct XmlRdfEnvelope {
    #[serde(rename = "Currency")]
    items: Vec<XmlItem>,
}

#[derive(Deserialize, Debug)]
struct XmlItem {
    #[serde(rename = "ForexBuying")]
    forex_buying: f64,
    #[serde(rename = "ForexSelling")]
    forex_selling: Option<f64>,
    #[serde(rename = "Unit")]
    unit: u64,
    #[serde(rename = "CurrencyCode")]
    currency_code: String,
}

impl IsForex for CentralBankOfTurkey {
    fn get_base_url(&self) -> &str {
        "https://www.tcmb.gov.tr/kurlar/202401/31012024.xml"
    }

    fn extract_rate(&self, bytes: &[u8], timestamp: u64) -> Result<super::ForexRateMap, ExtractError> {
        let data: XmlRdfEnvelope = serde_xml_rs::from_reader(bytes)
            .map_err(|e| ExtractError::XmlDeserialize(format!("{:?}", e)))?;
        ic_cdk::println!("Data: {:?}", data);
        let mut rate_map = data
            .items
            .iter()
            .filter_map(|item| {
              // Return the average of Forex buying and selling rates.
              if item.forex_selling.is_none() {
                  return Some((item.currency_code.clone(), item.forex_buying as u64));
              }
              else {
                  let rate = (item.forex_buying + item.forex_selling.unwrap()) / 2.0;
                  Some((item.currency_code.clone(), rate as u64))
              }
            })
            .collect::<super::ForexRateMap>();

        if rate_map.is_empty() {
            return Err(ExtractError::RateNotFound {
                filter: "No rates found".to_string(),
            });
        }

        rate_map.insert("TRY".to_string(), super::RATE_UNIT);

        Ok(rate_map)
    }

    fn get_utc_offset(&self) -> i16 {
        3
    }

    fn max_response_bytes(&self) -> u64 {
        500 * ONE_KIB
    }

    fn supports_ipv6(&self) -> bool {
        false
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
        let forex = Forex::CentralBankOfTurkey(CentralBankOfTurkey);
        assert_eq!(forex.to_string(), "CentralBankOfTurkey");
    }

    /// The function tests if the macro correctly generates derive copies by
    /// verifying that the forex sources return the correct query string.
    #[test]
    fn query_string() {
        let forex = CentralBankOfTurkey;
        assert_eq!(
            forex.get_url(0),
            "https://www.tcmb.gov.tr/kurlar/202401/31012024.xml"
        );
    }

    /// This function tests that the forex sources can report the max response bytes needed to make a successful HTTP outcall.
    #[test]
    fn max_response_bytes() {
        let forex = Forex::CentralBankOfTurkey(CentralBankOfTurkey);
        assert_eq!(forex.max_response_bytes(), 500 * ONE_KIB);
    }

    /// The function tests if the [CentralBankOfTurkey] struct returns the correct forex rate.
    #[test]
    fn extract_rate() {
        let forex = CentralBankOfTurkey;
        let query_response = load_file("test-data/forex/central-bank-of-turkey.xml");
        let timestamp = 1681171200;
        let extracted_rates = forex
            .extract_rate(&query_response, timestamp)
            .expect("Failed to extract rates");
    }
}