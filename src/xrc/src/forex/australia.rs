use chrono::NaiveDateTime;
use serde::Deserialize;

use crate::{ExtractError, ONE_KIB, RATE_UNIT};

use super::{ForexRateMap, IsForex, ReserveBankOfAustralia, SECONDS_PER_DAY};

#[derive(Deserialize, Debug)]
struct XmlRdfEnvelope {
    #[serde(rename = "item")]
    items: Vec<XmlItem>,
}

#[derive(Deserialize, Debug)]
struct XmlItem {
    statistics: XmlItemStatistics,
}

#[derive(Deserialize, Debug)]
struct XmlItemStatistics {
    #[serde(rename = "exchangeRate")]
    exchange_rate: XmlItemStatisticsExchangeRate,
}

#[derive(Deserialize, Debug)]
struct XmlItemStatisticsExchangeRate {
    #[serde(rename = "targetCurrency")]
    target_currency: String,
    observation: XmlItemStatisticsExchangeRateObservation,
    #[serde(rename = "observationPeriod")]
    observation_period: XmlItemStatisticsExchangeRateObservationPeriod,
}

#[derive(Deserialize, Debug)]
struct XmlItemStatisticsExchangeRateObservation {
    value: f64,
}

#[derive(Deserialize, Debug)]
struct XmlItemStatisticsExchangeRateObservationPeriod {
    frequency: String,
    period: String,
}

impl IsForex for ReserveBankOfAustralia {
    fn get_base_url(&self) -> &str {
        "https://www.rba.gov.au/rss/rss-cb-exchange-rates.xml"
    }

    fn extract_rate(&self, bytes: &[u8], timestamp: u64) -> Result<ForexRateMap, ExtractError> {
        let timestamp = (timestamp / SECONDS_PER_DAY) * SECONDS_PER_DAY;

        let data: XmlRdfEnvelope = serde_xml_rs::from_reader(bytes)
            .map_err(|e| ExtractError::XmlDeserialize(format!("{:?}", e)))?;

        let mut rate_map = data
            .items
            .iter()
            .filter_map(|item| {
                // Skip "XXX" currency code. Used for transaction where no currency is involved.
                if item.statistics.exchange_rate.target_currency == "XXX" {
                    return None;
                }

                // Skip observation periods that are not daily.
                if item.statistics.exchange_rate.observation_period.frequency != "daily" {
                    return None;
                }

                // Parse the observation period date.
                let period = format!(
                    "{} 00:00:00",
                    item.statistics.exchange_rate.observation_period.period
                );
                let extracted_timestamp =
                    NaiveDateTime::parse_from_str(&period, "%Y-%m-%d %H:%M:%S")
                        .unwrap_or_else(|e| {
                            println!("{:?}", e);
                            NaiveDateTime::from_timestamp(0, 0)
                        })
                        .timestamp() as u64;
                // Skip entries where timestamp does not match.
                if timestamp != extracted_timestamp {
                    return None;
                }
                let rate =
                    (item.statistics.exchange_rate.observation.value * RATE_UNIT as f64) as u64;
                Some((item.statistics.exchange_rate.target_currency.clone(), rate))
            })
            .collect::<ForexRateMap>();
        rate_map.insert("AUD".to_string(), RATE_UNIT);

        self.normalize_to_usd(&rate_map)
    }

    fn get_utc_offset(&self) -> i16 {
        10
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
    use maplit::hashmap;

    use super::*;

    use crate::{forex::Forex, utils::test::load_file};

    /// The function test if the macro correctly generates the
    /// [core::fmt::Display] trait's implementation for [Forex].
    #[test]
    fn to_string() {
        let forex = Forex::ReserveBankOfAustralia(ReserveBankOfAustralia);
        assert_eq!(forex.to_string(), "ReserveBankOfAustralia");
    }

    /// The function tests if the macro correctly generates derive copies by
    /// verifying that the forex sources return the correct query string.
    #[test]
    fn query_string() {
        let forex = ReserveBankOfAustralia;
        assert_eq!(
            forex.get_url(0),
            "https://www.rba.gov.au/rss/rss-cb-exchange-rates.xml"
        );
    }

    #[test]
    fn max_response_bytes() {
        let forex = Forex::ReserveBankOfAustralia(ReserveBankOfAustralia);
        assert_eq!(forex.max_response_bytes(), 500 * ONE_KIB);
    }

    #[test]
    fn extract_rate() {
        let forex = ReserveBankOfAustralia;
        let query_response = load_file("test-data/forex/reserve-bank-of-australia.xml");
        let timestamp = 1681171200;
        let extracted_rates = forex
            .extract_rate(&query_response, timestamp)
            .expect("should be able to extract rates");
        assert_eq!(
            extracted_rates,
            hashmap! {
                "INR".to_string() => 82_057_810_393,
                "IDR".to_string() => 14_876_441_515_650,
                "XDR".to_string() => 741_949_977,
                "AUD".to_string() => 1_497_678_598,
                "TWD".to_string() => 30_477_759_472,
                "JPY".to_string() => 133_263_441_665,
                "THB".to_string() => 34_296_839_898,
                "PHP".to_string() => 54_680_245_619,
                "GBP".to_string() => 805_301_782,
                "EUR".to_string() => 917_777_444,
                "SGD".to_string() => 1_330_687_434,
                "KRW".to_string() => 1_320_907_593_230,
                "NZD".to_string() => 1_605_361_689,
                "CHF".to_string() => 907_143_926,
                "USD".to_string() => 1_000_000_000,
                "MYR".to_string() => 4_416_504_418,
                "CNY".to_string() => 6_884_229_444,
                "HKD".to_string() => 7_849_932_604,
                "VND".to_string() => 23_449_153_811_592,
            }
        );
    }
}
