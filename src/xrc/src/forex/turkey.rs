use chrono::NaiveDateTime;
use serde::{de, Deserialize, Deserializer};

use crate::{ExtractError, ONE_DAY_SECONDS, ONE_KIB, RATE_UNIT};
use super::{IsForex, CentralBankOfTurkey};

#[derive(Deserialize, Debug)]
struct XmlRdfEnvelope {
    #[serde(rename = "Tarih")]
    date: String,
    #[serde(rename = "Currency")]
    items: Vec<XmlItem>,
}

#[derive(Deserialize, Debug)]
struct XmlItem {
    #[serde(rename = "ForexBuying", deserialize_with = "val_deserializer")]
    forex_buying: String,
    #[serde(rename = "ForexSelling", deserialize_with = "val_deserializer")]
    forex_selling: String,
    #[serde(rename = "Unit")]
    unit: f64,
    #[serde(rename = "CurrencyCode")]
    currency_code: String,
}

// Custom deserializer for handling empty tags in the XML to
// avoid serde_xml_rs UnexpectedToken errors.
fn val_deserializer<'de, D>(d: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(d)?;
    match &s[..] {
        "" => Ok("0.0".to_string()),
        _ => Ok(s.parse().map_err(de::Error::custom)?),
    }
}

impl IsForex for CentralBankOfTurkey {
    fn get_base_url(&self) -> &str {
        "https://www.tcmb.gov.tr/kurlar/YM/DMY.xml"
    }

    // Override the default implementation of [get_url] of the [IsForex] trait.
    fn get_url(&self, timestamp: u64) -> String {
        let year_month = NaiveDateTime::from_timestamp_opt(timestamp as i64, 0)
            .map(|t| t.format("%Y%m").to_string())
            .unwrap_or_default();
          
        let day_month_year = NaiveDateTime::from_timestamp_opt(timestamp as i64, 0)
            .map(|t| t.format("%d%m%Y").to_string())
            .unwrap_or_default();

        self.get_base_url()
            .replace("YM", &year_month)
            .replace("DMY", &day_month_year)
    }

    fn extract_rate(&self, bytes: &[u8], timestamp: u64) -> Result<super::ForexRateMap, ExtractError> {
        let timestamp = (timestamp / ONE_DAY_SECONDS) * ONE_DAY_SECONDS;

        let data: XmlRdfEnvelope = serde_xml_rs::from_reader(bytes)
            .map_err(|e| ExtractError::XmlDeserialize(format!("{:?}", e)))?;

        let date = format!("{} 00:00:00", data.date);
        let extracted_timestamp = NaiveDateTime::parse_from_str(&date, "%d.%m.%Y %H:%M:%S")
            .map(|t| t.timestamp())
            .unwrap_or_else(|_| {
                NaiveDateTime::from_timestamp_opt(0, 0)
                    .map(|t| t.timestamp())
                    .unwrap_or_default()
            }) as u64;

        if extracted_timestamp != timestamp {
            return Err(ExtractError::RateNotFound {
                filter: "Cannot find data for timestamp".to_string(),
            });
        }

        let mut rate_map = data
            .items
            .iter()
            .filter_map(|item| {
                  let buying = item.forex_buying.parse::<f64>().unwrap_or_default();
                  let selling = item.forex_selling.parse::<f64>().unwrap_or_default();
                  if buying == 0.0 || selling == 0.0 {
                      None
                  }
                  else {
                      // Return the average of the buying and selling rates.
                      let rate = (buying + selling) / 2.0;
                      Some((
                          item.currency_code.clone().to_uppercase(), 
                          (RATE_UNIT as f64 * rate / item.unit) as u64
                      ))
                  }
            })
            .collect::<super::ForexRateMap>();

        if rate_map.is_empty() {
            return Err(ExtractError::RateNotFound {
                filter: "No rates found".to_string(),
            });
        }

        rate_map.insert("TRY".to_string(), RATE_UNIT);

        self.normalize_to_usd(&rate_map)
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
        let timestamp: u64 = 1706677200;
        let forex = CentralBankOfTurkey;
        assert_eq!(
            forex.get_url(timestamp),
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
        let timestamp = 1706677200;
        let extracted_rates = forex.extract_rate(&query_response, timestamp);

        assert!(matches!(extracted_rates, Ok(ref rates) if rates["CHF"] == 1_158_346_373));
        assert!(matches!(extracted_rates, Ok(ref rates) if rates["JPY"] == 6_768_796));
    }
}