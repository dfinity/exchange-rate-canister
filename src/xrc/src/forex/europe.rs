use chrono::NaiveDateTime;
use serde::Deserialize;

use crate::{ExtractError, ONE_KIB, RATE_UNIT};

use super::{EuropeanCentralBank, ForexRateMap, IsForex, ONE_DAY_SECONDS};

#[derive(Deserialize, Debug)]
enum XmlEcbOptions {
    #[serde(rename = "subject")]
    Subject(()),
    #[serde(rename = "Sender")]
    Sender(()),
    Cube(XmlEcbOuterCube),
}

#[derive(Deserialize, Debug)]
struct XmlEcbSender {
    #[serde(rename = "name")]
    _name: String,
}

#[derive(Deserialize, Debug)]
struct XmlEcbCubeObject {
    time: String,
    #[serde(rename = "$value")]
    cubes: Vec<XmlEcbCube>,
}

#[derive(Deserialize, Debug)]
struct XmlEcbCube {
    currency: String,
    rate: f64,
}

#[derive(Deserialize, Debug)]
struct XmlEcbOuterCube {
    #[serde(rename = "Cube")]
    cube: XmlEcbCubeObject,
}

#[derive(Deserialize, Debug)]
struct XmlEcbEnvelope {
    #[serde(rename = "$value")]
    entries: Vec<XmlEcbOptions>,
}

/// European Central Bank
impl IsForex for EuropeanCentralBank {
    fn format_timestamp(&self, _timestamp: u64) -> String {
        // ECB does not take a timestamp/date as an argument. It always returns the latest date.
        "".to_string()
    }

    fn extract_rate(&self, bytes: &[u8], timestamp: u64) -> Result<ForexRateMap, ExtractError> {
        let timestamp = (timestamp / ONE_DAY_SECONDS) * ONE_DAY_SECONDS;

        let data: XmlEcbEnvelope = serde_xml_rs::from_reader(bytes)
            .map_err(|e| ExtractError::XmlDeserialize(format!("{:?}", e)))?;

        if let XmlEcbOptions::Cube(cubes) = data
            .entries
            .iter()
            .find(|e| matches!(e, XmlEcbOptions::Cube(_)))
            .unwrap_or(&XmlEcbOptions::Cube(XmlEcbOuterCube {
                cube: XmlEcbCubeObject {
                    time: "0".to_string(),
                    cubes: vec![],
                },
            }))
        {
            let extracted_timestamp = NaiveDateTime::parse_from_str(
                &(cubes.cube.time.clone() + " 00:00:00"),
                "%Y-%m-%d %H:%M:%S",
            )
            .map(|t| t.timestamp())
            .unwrap_or_else(|_| {
                NaiveDateTime::from_timestamp_opt(0, 0)
                    .map(|t| t.timestamp())
                    .unwrap_or_default()
            }) as u64;

            if extracted_timestamp != timestamp {
                Err(ExtractError::RateNotFound {
                    filter: "Invalid timestamp".to_string(),
                })
            } else {
                let mut values: ForexRateMap = cubes
                    .cube
                    .cubes
                    .iter()
                    .map(|cube| {
                        (
                            cube.currency.to_uppercase(),
                            ((1.0 / cube.rate) * RATE_UNIT as f64) as u64,
                        )
                    })
                    .collect();
                values.insert("EUR".to_string(), RATE_UNIT);
                self.normalize_to_usd(&values)
            }
        } else {
            Err(ExtractError::XmlDeserialize(
                String::from_utf8(bytes.to_vec()).unwrap_or_default(),
            ))
        }
    }

    fn get_base_url(&self) -> &str {
        "https://www.ecb.europa.eu/stats/eurofxref/eurofxref-daily.xml"
    }

    fn get_utc_offset(&self) -> i16 {
        1
    }

    fn max_response_bytes(&self) -> u64 {
        ONE_KIB * 3
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
        let forex = Forex::EuropeanCentralBank(EuropeanCentralBank);
        assert_eq!(forex.to_string(), "EuropeanCentralBank");
    }

    /// The function tests if the macro correctly generates derive copies by
    /// verifying that the forex sources return the correct query string.
    #[test]
    fn get_url() {
        let timestamp = 1661524016;
        let ecb = EuropeanCentralBank;
        let query_string = ecb.get_url(timestamp);
        assert_eq!(
            query_string,
            "https://www.ecb.europa.eu/stats/eurofxref/eurofxref-daily.xml"
        );
    }

    /// The function tests if the [EuropeanCentralBank] struct returns the correct forex rate.
    #[test]
    fn extract_rate() {
        let ecb = EuropeanCentralBank;
        let query_response = load_file("test-data/forex/central-bank-of-europe.xml");
        let timestamp: u64 = 1664755200;
        let extracted_rates = ecb.extract_rate(&query_response, timestamp);

        assert!(matches!(extracted_rates, Ok(rates) if rates["EUR"] == 976_400_000));
    }

    /// This function tests that the forex sources can report the max response bytes needed to make a successful HTTP outcall.
    #[test]
    fn max_response_bytes() {
        let forex = Forex::EuropeanCentralBank(EuropeanCentralBank);
        assert_eq!(forex.max_response_bytes(), 3 * ONE_KIB);
    }
}
