use chrono::NaiveDateTime;
use serde::Deserialize;

use crate::{ExtractError, ONE_KIB, RATE_UNIT};

use super::{ForexRateMap, IsForex, ReserveBankOfAustralia, ONE_DAY_SECONDS};

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
        let timestamp = (timestamp / ONE_DAY_SECONDS) * ONE_DAY_SECONDS;

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
                // TODO(DEFI-2648): Migrate to non-deprecated.
                #[allow(deprecated)]
                let extracted_timestamp =
                    NaiveDateTime::parse_from_str(&period, "%Y-%m-%d %H:%M:%S")
                        .map(|t| t.timestamp())
                        .unwrap_or_else(|_| {
                            NaiveDateTime::from_timestamp_opt(0, 0)
                                .map(|t| t.timestamp())
                                .unwrap_or_default()
                        }) as u64;
                // Skip entries where timestamp does not match.
                if timestamp != extracted_timestamp {
                    return None;
                }
                // The RBA feed quotes AUD as the base currency, so each value is
                // the target currency per 1 AUD (e.g. 0.6677 USD per AUD). The
                // `normalize_to_usd` step expects rates in the opposite
                // orientation, i.e. AUD per 1 unit of the target currency, so the
                // value is inverted here before being collected.
                let value = item.statistics.exchange_rate.observation.value;
                if value <= 0.0 {
                    return None;
                }
                let rate = (RATE_UNIT as f64 / value) as u64;
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
    use maplit::btreemap;

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

    /// This function tests that the forex sources can report the max response bytes needed to make a successful HTTP outcall.
    #[test]
    fn max_response_bytes() {
        let forex = Forex::ReserveBankOfAustralia(ReserveBankOfAustralia);
        assert_eq!(forex.max_response_bytes(), 500 * ONE_KIB);
    }

    /// The function tests if the [ReserveBankOfAustralia] struct returns the correct forex rate.
    #[test]
    fn extract_rate() {
        let forex = ReserveBankOfAustralia;
        let query_response = load_file("test-data/forex/reserve-bank-of-australia.xml");
        let timestamp = 1681171200;
        let extracted_rates = forex
            .extract_rate(&query_response, timestamp)
            .expect("should be able to extract rates");
        // Rates are USD per 1 unit of the listed currency (scaled by RATE_UNIT),
        // so non-USD currencies that are worth less than a USD are below
        // RATE_UNIT and those worth more are above it. For example one USD is
        // worth ~0.67 USD-equivalent (AUD), and one VND is worth ~0.0000426 USD.
        assert_eq!(
            extracted_rates,
            btreemap! {
                "INR".to_string() => 12_186_529,
                "IDR".to_string() => 67_220,
                "XDR".to_string() => 1_347_799_757,
                "AUD".to_string() => 667_700_000,
                "TWD".to_string() => 32_810_810,
                "JPY".to_string() => 7_503_933,
                "THB".to_string() => 29_157_205,
                "PHP".to_string() => 18_288_140,
                "GBP".to_string() => 1_241_770_503,
                "EUR".to_string() => 1_089_588_772,
                "SGD".to_string() => 751_491_276,
                "KRW".to_string() => 757_054,
                "NZD".to_string() => 622_912_585,
                "CHF".to_string() => 1_102_360_904,
                "USD".to_string() => 1_000_000_000,
                "MYR".to_string() => 226_423_411,
                "CNY".to_string() => 145_259_539,
                "HKD".to_string() => 127_389_628,
                "VND".to_string() => 42_645,
            }
        );
    }
}
