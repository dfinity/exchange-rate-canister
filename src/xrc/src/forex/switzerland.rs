use chrono::NaiveDateTime;
use serde::Deserialize;

use crate::{ExtractError, ONE_KIB, RATE_UNIT};

use super::{ForexRateMap, IsForex, SwissFederalOfficeForCustoms, SECONDS_PER_DAY};

#[derive(Deserialize, Debug)]
struct XmlRdfEnvelope {
    datum: String,
    #[serde(rename = "devise")]
    items: Vec<XmlItem>,
}

#[derive(Deserialize, Debug)]
struct XmlItem {
    code: String,
    waehrung: String,
    kurs: String,
}

impl IsForex for SwissFederalOfficeForCustoms {
    fn format_timestamp(&self, timestamp: u64) -> String {
        format!(
            "{}",
            NaiveDateTime::from_timestamp(timestamp.try_into().unwrap_or(0), 0).format("%Y%m%d")
        )
    }

    fn get_base_url(&self) -> &str {
        "https://www.backend-rates.bazg.admin.ch/api/xmldaily?d=DATE&locale=en"
    }

    fn extract_rate(&self, bytes: &[u8], timestamp: u64) -> Result<ForexRateMap, ExtractError> {
        let timestamp = (timestamp / SECONDS_PER_DAY) * SECONDS_PER_DAY;

        let data: XmlRdfEnvelope = serde_xml_rs::from_reader(bytes)
            .map_err(|e| ExtractError::XmlDeserialize(format!("{:?}", e)))?;

        let date = format!("{} 00:00:00", data.datum);
        let extracted_timestamp = NaiveDateTime::parse_from_str(&date, "%d.%m.%Y %H:%M:%S")
            .unwrap_or_else(|_| NaiveDateTime::from_timestamp(0, 0))
            .timestamp() as u64;

        if extracted_timestamp != timestamp {
            return Err(ExtractError::RateNotFound {
                filter: "Cannot find data for timestamp".to_string(),
            });
        }

        let mut rate_map = data
            .items
            .iter()
            .filter_map(|item| {
                let units = item.waehrung.split(' ').next();
                units?;

                let value = item.kurs.parse::<f64>();
                let units = units.unwrap().parse::<f64>();
                match (value, units) {
                    (Ok(value), Ok(units)) => Some((item.code.to_uppercase(), (RATE_UNIT as f64 * value / units) as u64)),
                    _ => None,
                }
            })
            .collect::<ForexRateMap>();
        rate_map.insert("CHF".to_string(), RATE_UNIT);

        self.normalize_to_usd(&rate_map)
    }

    fn get_utc_offset(&self) -> i16 {
        1
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
        let forex = Forex::SwissFederalOfficeForCustoms(SwissFederalOfficeForCustoms);
        assert_eq!(forex.to_string(), "SwissFederalOfficeForCustoms");
    }

    /// The function tests if the macro correctly generates derive copies by
    /// verifying that the forex sources return the correct query string.
    #[test]
    fn query_string() {
        let forex = SwissFederalOfficeForCustoms;
        let timestamp = 1661524016;
        assert_eq!(
            forex.get_url(timestamp),
            "https://www.backend-rates.bazg.admin.ch/api/xmldaily?d=20220826&locale=en"
        );
    }

    /// This function tests that the forex sources can report the max response bytes needed to make a successful HTTP outcall.
    #[test]
    fn max_response_bytes() {
        let forex = Forex::SwissFederalOfficeForCustoms(SwissFederalOfficeForCustoms);
        assert_eq!(forex.max_response_bytes(), 500 * ONE_KIB);
    }

    /// The function tests if the [SwissFederalOfficeForCustoms] struct returns the correct forex rate.
    #[test]
    fn extract_rate() {
        let forex = SwissFederalOfficeForCustoms;
        let query_response = load_file("test-data/forex/swiss-office-for-customs.xml");
        let timestamp: u64 = 1656374400;
        let extracted_rates = forex.extract_rate(&query_response, timestamp);
        assert!(matches!(extracted_rates, Ok(ref rates) if rates["EUR"] == 1_057_421_866));
        assert!(matches!(extracted_rates, Ok(ref rates) if rates["JPY"] == 7_399_602));
    }
}
