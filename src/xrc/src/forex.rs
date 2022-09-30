use ::candid::Deserialize;
use chrono::naive::NaiveDateTime;
use jaq_core::Val;
use std::str::FromStr;
use std::{collections::HashMap, convert::TryInto};

use crate::jq;
use crate::ExtractError;

type ForexRateMap = HashMap<String, u64>;

const SECONDS_PER_DAY: u64 = 60 * 60 * 24;

/// This macro generates the necessary boilerplate when adding a forex data source to this module.
macro_rules! forex {
    ($($name:ident),*) => {
        /// Enum that contains all of the possible forex sources.
        pub enum Forex {
            $(
                #[allow(missing_docs)]
                #[allow(dead_code)]
                $name($name),
            )*
        }

        $(pub struct $name;)*

        impl core::fmt::Display for Forex {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $(Forex::$name(_) => write!(f, stringify!($name))),*,
                }
            }
        }

        /// Contains all of the known forex sources that can be found in the
        /// [Forex] enum.
        #[allow(dead_code)]
        pub const FOREX_SOURCES: &'static [Forex] = &[
            $(Forex::$name($name)),*
        ];


        /// Implements the core functionality of the generated `Forex` enum.
        impl Forex {

            /// This method routes the request to the correct forex's [IsForex::get_url] method.
            #[allow(dead_code)]
            pub fn get_url(&self, timestamp: u64) -> String {
                match self {
                    $(Forex::$name(forex) => forex.get_url(timestamp)),*,
                }
            }

            /// This method routes the response's body and the timestamp to the correct forex's
            /// [IsForex::extract_rate].
            #[allow(dead_code)]
            pub fn extract_rate(&self, bytes: &[u8], timestamp: u64) -> Result<ForexRateMap, ExtractError> {
                match self {
                    $(Forex::$name(forex) => forex.extract_rate(bytes, timestamp)),*,
                }
            }
        }
    }

}

forex! { MonetaryAuthorityOfSingapore, CentralBankOfMyanmar, CentralBankOfBosniaHerzegovina, BankOfIsrael }

/// The base URL may contain the following placeholders:
/// `DATE`: This string must be replaced with the timestamp string as provided by `format_timestamp`.
const DATE: &str = "DATE";

/// This trait is use to provide the basic methods needed for a forex data source.
trait IsForex {
    /// The base URL template that is provided to [IsForex::get_url].
    fn get_base_url(&self) -> &str;

    /// Provides the ability to format the timestamp as a string. Default implementation is
    /// to simply return the provided timestamp as a string.
    fn format_timestamp(&self, timestamp: u64) -> String {
        timestamp.to_string()
    }

    /// A default implementation to generate a URL based on the given parameters.
    /// The method takes the base URL for the forex and replaces the following
    /// placeholders:
    /// * [DATE]
    fn get_url(&self, timestamp: u64) -> String {
        let timestamp = (timestamp / SECONDS_PER_DAY) * SECONDS_PER_DAY;
        self.get_base_url()
            .replace(DATE, &self.format_timestamp(timestamp))
    }

    /// A default implementation to extract the rate from the response's body
    /// using the base filter and [jq::extract].
    fn extract_rate(&self, bytes: &[u8], timestamp: u64) -> Result<ForexRateMap, ExtractError>;

    /// A utility function that receives a set of rates relative to some quote asset, and returns a set of rates relative to USD as the quote asset
    fn normalize_to_usd(&self, values: &ForexRateMap) -> Result<ForexRateMap, ExtractError> {
        match values.get("usd") {
            Some(usd_value) => Ok(values
                .iter()
                .map(|(symbol, value)| {
                    (
                        symbol.to_string(),
                        ((10_000.0 * (*value as f64)) / (*usd_value as f64)) as u64,
                    )
                })
                .collect()),
            None => Err(ExtractError::RateNotFound {
                filter: "No USD rate".to_string(),
            }),
        }
    }
}

/// Monetary Authority Of Singapore
impl IsForex for MonetaryAuthorityOfSingapore {
    fn format_timestamp(&self, timestamp: u64) -> String {
        format!(
            "{}",
            NaiveDateTime::from_timestamp(timestamp.try_into().unwrap_or(0), 0).format("%Y-%m-%d")
        )
    }

    fn extract_rate(&self, bytes: &[u8], timestamp: u64) -> Result<ForexRateMap, ExtractError> {
        let timestamp = (timestamp / SECONDS_PER_DAY) * SECONDS_PER_DAY;

        let filter = ".result.records[0]";
        let values = jq::extract(bytes, filter)?;
        match values {
            Val::Obj(obj) => {
                let mut extracted_timestamp = 0;
                let mut values = obj
                    .iter()
                    .filter_map(|(key, value)| {
                        match value {
                            Val::Str(s) => {
                                if key.to_string() == "end_of_day" {
                                    // The end_of_day entry tells us the date these rates were reported for
                                    extracted_timestamp = NaiveDateTime::parse_from_str(
                                        &(s.to_string() + " 00:00:00"),
                                        "%Y-%m-%d %H:%M:%S",
                                    )
                                    .unwrap_or_else(|_| NaiveDateTime::from_timestamp(0, 0))
                                    .timestamp()
                                        as u64;
                                    None
                                } else if !key.to_string().contains("_sgd") {
                                    // There are some other entries that do not contain _sgd or end_of_day and we do not care about them
                                    None
                                } else {
                                    match f64::from_str(&s.to_string()) {
                                        Ok(rate) => {
                                            let symbol_opt = key.split('_').next();
                                            match symbol_opt {
                                                Some(symbol) => {
                                                    if key.to_string().ends_with("_100") {
                                                        Some((
                                                            symbol.to_string(),
                                                            (rate * 100.0) as u64,
                                                        ))
                                                    } else {
                                                        Some((
                                                            symbol.to_string(),
                                                            (rate * 10_000.0) as u64,
                                                        ))
                                                    }
                                                }
                                                _ => None,
                                            }
                                        }
                                        _ => None,
                                    }
                                }
                            }
                            _ => None,
                        }
                    })
                    .collect::<ForexRateMap>();
                values.insert("sgd".to_string(), 10_000);
                if extracted_timestamp == timestamp {
                    self.normalize_to_usd(&values)
                } else {
                    Err(ExtractError::RateNotFound {
                        filter: "Invalid timestamp".to_string(),
                    })
                }
            }
            _ => Err(ExtractError::JsonDeserialize(
                "Not a valid object".to_string(),
            )),
        }
    }

    fn get_base_url(&self) -> &str {
        "https://eservices.mas.gov.sg/api/action/datastore/search.json?resource_id=95932927-c8bc-4e7a-b484-68a66a24edfe&limit=100&filters[end_of_day]=DATE"
    }
}

/// Central Bank of Myanmar
impl IsForex for CentralBankOfMyanmar {
    fn format_timestamp(&self, timestamp: u64) -> String {
        format!(
            "{}",
            NaiveDateTime::from_timestamp(timestamp.try_into().unwrap_or(0), 0).format("%d-%m-%Y")
        )
    }

    fn extract_rate(&self, bytes: &[u8], timestamp: u64) -> Result<ForexRateMap, ExtractError> {
        let timestamp = (timestamp / SECONDS_PER_DAY) * SECONDS_PER_DAY;

        let values = jq::extract(bytes, ".rates")?;
        let timestamp_jq = jq::extract(bytes, ".timestamp")?;
        let extracted_timestamp: u64 = match timestamp_jq {
            Val::Int(ref rc) => u64::try_from(*rc).unwrap_or(0),
            _ => 0,
        };
        if extracted_timestamp != timestamp {
            Err(ExtractError::RateNotFound {
                filter: "Invalid timestamp".to_string(),
            })
        } else {
            match values {
                Val::Obj(obj) => {
                    let values = obj
                        .iter()
                        .filter_map(|(key, value)| match value {
                            Val::Str(s) => match f64::from_str(&s.to_string().replace(',', "")) {
                                Ok(rate) => {
                                    Some((key.to_string().to_lowercase(), (rate * 10_000.0) as u64))
                                }
                                _ => None,
                            },
                            _ => None,
                        })
                        .collect::<ForexRateMap>();
                    self.normalize_to_usd(&values)
                }
                _ => Err(ExtractError::JsonDeserialize(
                    "Not a valid object".to_string(),
                )),
            }
        }
    }

    fn get_base_url(&self) -> &str {
        "https://forex.cbm.gov.mm/api/history/DATE"
    }
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
        let timestamp = (timestamp / SECONDS_PER_DAY) * SECONDS_PER_DAY;

        let values = jq::extract(bytes, ".CurrencyExchangeItems")?;
        let timestamp_jq = jq::extract(bytes, ".Date")?;
        let extracted_timestamp: u64 = match timestamp_jq {
            Val::Str(rc) => NaiveDateTime::parse_from_str(&(rc.to_string()), "%Y-%m-%dT%H:%M:%S")
                .unwrap_or_else(|_| NaiveDateTime::from_timestamp(0, 0))
                .timestamp() as u64,
            _ => 0,
        };
        if extracted_timestamp != timestamp {
            Err(ExtractError::RateNotFound {
                filter: "Invalid timestamp".to_string(),
            })
        } else {
            match values {
                Val::Arr(arr) => {
                    let values = arr
                        .iter()
                        .filter_map(|item| match item {
                            Val::Obj(obj) => {
                                let asset = match obj.get(&"AlphaCode".to_string()) {
                                    Some(Val::Str(s)) => Some(s.to_string()),
                                    _ => None,
                                };
                                let units = match obj.get(&"Units".to_string()) {
                                    Some(Val::Str(s)) => match u64::from_str(s.as_str()) {
                                        Ok(val) => Some(val),
                                        _ => None,
                                    },
                                    _ => None,
                                };
                                let rate = match obj.get(&"Middle".to_string()) {
                                    Some(Val::Str(s)) => match f64::from_str(s.as_str()) {
                                        Ok(val) => Some(val),
                                        _ => None,
                                    },
                                    _ => None,
                                };
                                if let (Some(asset), Some(units), Some(rate)) = (asset, units, rate)
                                {
                                    Some((
                                        asset.to_lowercase(),
                                        (rate * 10_000.0 / units as f64) as u64,
                                    ))
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        })
                        .collect::<ForexRateMap>();
                    self.normalize_to_usd(&values)
                }
                _ => Err(ExtractError::JsonDeserialize(format!(
                    "Not a valid object ({:?})",
                    values
                ))),
            }
        }
    }

    fn get_base_url(&self) -> &str {
        "https://www.cbbh.ba/CurrencyExchange/GetJson?date=DATE%2000%3A00%3A00"
    }
}

// Bank of Israel

// The following structs are used to parse the XML content provided by this forex data source.

#[derive(Deserialize, Debug)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum XmlBankOfIsraelCurrenciesOptions {
    LastUpdate(String),
    Currency(XmlBankOfIsraelCurrency),
}

#[derive(Deserialize, Debug)]
struct XmlBankOfIsraelCurrencies {
    #[serde(rename = "$value")]
    entries: Vec<XmlBankOfIsraelCurrenciesOptions>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
struct XmlBankOfIsraelCurrency {
    unit: u64,
    currencycode: String,
    rate: f64,
}

/// Bank of Israel
impl IsForex for BankOfIsrael {
    fn format_timestamp(&self, timestamp: u64) -> String {
        format!(
            "{}",
            NaiveDateTime::from_timestamp(timestamp.try_into().unwrap_or(0), 0).format("%Y%m%d")
        )
    }

    fn extract_rate(&self, bytes: &[u8], timestamp: u64) -> Result<ForexRateMap, ExtractError> {
        let timestamp = (timestamp / SECONDS_PER_DAY) * SECONDS_PER_DAY;

        let data: XmlBankOfIsraelCurrencies = serde_xml_rs::from_reader(bytes).map_err(|_| {
            ExtractError::XmlDeserialize(String::from_utf8(bytes.to_vec()).unwrap_or_default())
        })?;

        let values: Vec<&XmlBankOfIsraelCurrency> = data
            .entries
            .iter()
            .filter_map(|entry| match entry {
                XmlBankOfIsraelCurrenciesOptions::Currency(currency) => Some(currency),
                _ => None,
            })
            .collect();

        let extracted_timestamp = data
            .entries
            .iter()
            .find(|entry| matches!(entry, XmlBankOfIsraelCurrenciesOptions::LastUpdate(_)))
            .and_then(|entry| match entry {
                XmlBankOfIsraelCurrenciesOptions::LastUpdate(s) => Some(
                    NaiveDateTime::parse_from_str(
                        &(s.to_string() + " 00:00:00"),
                        "%Y-%m-%d %H:%M:%S",
                    )
                    .unwrap_or_else(|_| NaiveDateTime::from_timestamp(0, 0))
                    .timestamp() as u64,
                ),
                _ => None,
            })
            .unwrap_or(0);

        if extracted_timestamp != timestamp {
            Err(ExtractError::RateNotFound {
                filter: "Invalid timestamp".to_string(),
            })
        } else {
            let values = values
                .iter()
                .map(|item| {
                    (
                        item.currencycode.to_lowercase(),
                        (item.rate * 10_000.0) as u64 / item.unit,
                    )
                })
                .collect::<ForexRateMap>();
            self.normalize_to_usd(&values)
        }
    }

    fn get_base_url(&self) -> &str {
        "https://www.boi.org.il/currency.xml?rdate=DATE"
    }
}

#[cfg(test)]
mod test {
    use super::*;

    /// The function test if the macro correctly generates the
    /// [core::fmt::Display] trait's implementation for [Forex].
    #[test]
    fn forex_to_string_returns_name() {
        let forex = Forex::MonetaryAuthorityOfSingapore(MonetaryAuthorityOfSingapore);
        assert_eq!(forex.to_string(), "MonetaryAuthorityOfSingapore");
        let forex = Forex::CentralBankOfMyanmar(CentralBankOfMyanmar);
        assert_eq!(forex.to_string(), "CentralBankOfMyanmar");
        let forex = Forex::CentralBankOfBosniaHerzegovina(CentralBankOfBosniaHerzegovina);
        assert_eq!(forex.to_string(), "CentralBankOfBosniaHerzegovina");
        let forex = Forex::BankOfIsrael(BankOfIsrael);
        assert_eq!(forex.to_string(), "BankOfIsrael");
    }

    /// The function tests if the macro correctly generates derive copies by
    /// verifying that the forex sources return the correct query string.
    #[test]
    fn query_string_test() {
        // Note that the hours/minutes/seconds are ignored, setting the considered timestamp to 1661472000.
        let timestamp = 1661524016;
        let singapore = MonetaryAuthorityOfSingapore;
        let query_string = singapore.get_url(timestamp);
        assert_eq!(query_string, "https://eservices.mas.gov.sg/api/action/datastore/search.json?resource_id=95932927-c8bc-4e7a-b484-68a66a24edfe&limit=100&filters[end_of_day]=2022-08-26");
        let myanmar = CentralBankOfMyanmar;
        let query_string = myanmar.get_url(timestamp);
        assert_eq!(
            query_string,
            "https://forex.cbm.gov.mm/api/history/26-08-2022"
        );
        let bosnia = CentralBankOfBosniaHerzegovina;
        let query_string = bosnia.get_url(timestamp);
        assert_eq!(
            query_string,
            "https://www.cbbh.ba/CurrencyExchange/GetJson?date=08-26-2022%2000%3A00%3A00"
        );
        let israel = BankOfIsrael;
        let query_string = israel.get_url(timestamp);
        assert_eq!(
            query_string,
            "https://www.boi.org.il/currency.xml?rdate=20220826"
        );
    }

    /// The function tests if the [MonetaryAuthorityOfSingapore] struct returns the correct forex rate.
    #[test]
    fn extract_rate_from_singapore() {
        let singapore = MonetaryAuthorityOfSingapore;
        let query_response = "{\"success\": true,\"result\": {\"resource_id\": [\"95932927-c8bc-4e7a-b484-68a66a24edfe\"],\"limit\": 10,\"total\": \"1\",\"records\": [{\"end_of_day\": \"2022-06-28\",\"preliminary\": \"0\",\"eur_sgd\": \"1.4661\",\"gbp_sgd\": \"1.7007\",\"usd_sgd\": \"1.3855\",\"aud_sgd\": \"0.9601\",\"cad_sgd\": \"1.0770\",\"cny_sgd_100\": \"20.69\",\"hkd_sgd_100\": \"17.66\",\"inr_sgd_100\": \"1.7637\",\"idr_sgd_100\": \"0.009338\",\"jpy_sgd_100\": \"1.0239\",\"krw_sgd_100\": \"0.1078\",\"myr_sgd_100\": \"31.50\",\"twd_sgd_100\": \"4.6694\",\"nzd_sgd\": \"0.8730\",\"php_sgd_100\": \"2.5268\",\"qar_sgd_100\": \"37.89\",\"sar_sgd_100\": \"36.91\",\"chf_sgd\": \"1.4494\",\"thb_sgd_100\": \"3.9198\",\"aed_sgd_100\": \"37.72\",\"vnd_sgd_100\": \"0.005959\",\"timestamp\": \"1663273633\"}]}}"
            .as_bytes();
        let timestamp: u64 = 1656374400;
        let extracted_rates = singapore.extract_rate(query_response, timestamp);

        assert!(matches!(extracted_rates, Ok(rates) if rates["eur"] == 10_581));
    }

    /// The function tests if the [CentralBankOfMyanmar] struct returns the correct forex rate.
    #[test]
    fn extract_rate_from_myanmar() {
        let myanmar = CentralBankOfMyanmar;
        let query_response = "{\"info\": \"Central Bank of Myanmar\",\"description\": \"Official Website of Central Bank of Myanmar\",\"timestamp\": 1656374400,\"rates\": {\"USD\": \"1,850.0\",\"VND\": \"7.9543\",\"THB\": \"52.714\",\"SEK\": \"184.22\",\"LKR\": \"5.1676\",\"ZAR\": \"116.50\",\"RSD\": \"16.685\",\"SAR\": \"492.89\",\"RUB\": \"34.807\",\"PHP\": \"33.821\",\"PKR\": \"8.9830\",\"NOK\": \"189.43\",\"NZD\": \"1,165.9\",\"NPR\": \"14.680\",\"MYR\": \"420.74\",\"LAK\": \"12.419\",\"KWD\": \"6,033.2\",\"KRW\": \"144.02\",\"KES\": \"15.705\",\"ILS\": \"541.03\",\"IDR\": \"12.469\",\"INR\": \"23.480\",\"HKD\": \"235.76\",\"EGP\": \"98.509\",\"DKK\": \"263.36\",\"CZK\": \"79.239\",\"CNY\": \"276.72\",\"CAD\": \"1,442.7\",\"KHR\": \"45.488\",\"BND\": \"1,335.6\",\"BRL\": \"353.14\",\"BDT\": \"19.903\",\"AUD\": \"1,287.6\",\"JPY\": \"1,363.4\",\"CHF\": \"1,937.7\",\"GBP\": \"2,272.0\",\"SGD\": \"1,335.6\",\"EUR\": \"1,959.7\"}}"
            .as_bytes();
        let timestamp: u64 = 1656374400;
        let extracted_rates = myanmar.extract_rate(query_response, timestamp);

        assert!(matches!(extracted_rates, Ok(rates) if rates["eur"] == 10_592));
    }

    /// The function tests if the [CentralBankOfBosniaHerzegovina] struct returns the correct forex rate.
    #[test]
    fn extract_rate_from_bosnia() {
        let bosnia = CentralBankOfBosniaHerzegovina;
        let query_response = "{\"CurrencyExchangeItems\": [{\"Country\": \"EMU\",\"NumCode\": \"978\",\"AlphaCode\": \"EUR\",\"Units\": \"1\",\"Buy\": \"1.955830\",\"Middle\": \"1.955830\",\"Sell\": \"1.955830\",\"Star\": null},{\"Country\": \"Australia\",\"NumCode\": \"036\",\"AlphaCode\": \"AUD\",\"Units\": \"1\",\"Buy\": \"1.276961\",\"Middle\": \"1.280161\",\"Sell\": \"1.283361\",\"Star\": null},{\"Country\": \"Canada\",\"NumCode\": \"124\",\"AlphaCode\": \"CAD\",\"Units\": \"1\",\"Buy\": \"1.430413\",\"Middle\": \"1.433998\",\"Sell\": \"1.437583\",\"Star\": null},{\"Country\": \"Croatia\",\"NumCode\": \"191\",\"AlphaCode\": \"HRK\",\"Units\": \"100\",\"Buy\": \"25.897554\",\"Middle\": \"25.962460\",\"Sell\": \"26.027366\",\"Star\": null},{\"Country\": \"Czech R\",\"NumCode\": \"203\",\"AlphaCode\": \"CZK\",\"Units\": \"1\",\"Buy\": \"0.078909\",\"Middle\": \"0.079107\",\"Sell\": \"0.079305\",\"Star\": null},{\"Country\": \"Dennmark\",\"NumCode\": \"208\",\"AlphaCode\": \"DKK\",\"Units\": \"1\",\"Buy\": \"0.262195\",\"Middle\": \"0.262852\",\"Sell\": \"0.263509\",\"Star\": null},{\"Country\": \"Hungary\",\"NumCode\": \"348\",\"AlphaCode\": \"HUF\",\"Units\": \"100\",\"Buy\": \"0.484562\",\"Middle\": \"0.485776\",\"Sell\": \"0.486990\",\"Star\": null},{\"Country\": \"Japan\",\"NumCode\": \"392\",\"AlphaCode\": \"JPY\",\"Units\": \"100\",\"Buy\": \"1.361913\",\"Middle\": \"1.365326\",\"Sell\": \"1.368739\",\"Star\": null},{\"Country\": \"Norway\",\"NumCode\": \"578\",\"AlphaCode\": \"NOK\",\"Units\": \"1\",\"Buy\": \"0.187446\",\"Middle\": \"0.187916\",\"Sell\": \"0.188386\",\"Star\": null},{\"Country\": \"Sweden\",\"NumCode\": \"752\",\"AlphaCode\": \"SEK\",\"Units\": \"1\",\"Buy\": \"0.182821\",\"Middle\": \"0.183279\",\"Sell\": \"0.183737\",\"Star\": null},{\"Country\": \"Switzerland\",\"NumCode\": \"756\",\"AlphaCode\": \"CHF\",\"Units\": \"1\",\"Buy\": \"1.923435\",\"Middle\": \"1.928256\",\"Sell\": \"1.933077\",\"Star\": null},{\"Country\": \"Turkey\",\"NumCode\": \"949\",\"AlphaCode\": \"TRY\",\"Units\": \"1\",\"Buy\": \"0.111613\",\"Middle\": \"0.111893\",\"Sell\": \"0.112173\",\"Star\": null},{\"Country\": \"G.Britain\",\"NumCode\": \"826\",\"AlphaCode\": \"GBP\",\"Units\": \"1\",\"Buy\": \"2.263272\",\"Middle\": \"2.268944\",\"Sell\": \"2.274616\",\"Star\": null},{\"Country\": \"USA\",\"NumCode\": \"840\",\"AlphaCode\": \"USD\",\"Units\": \"1\",\"Buy\": \"1.845384\",\"Middle\": \"1.850009\",\"Sell\": \"1.854634\",\"Star\": null},{\"Country\": \"Russia\",\"NumCode\": \"643\",\"AlphaCode\": \"RUB\",\"Units\": \"1\",\"Buy\": \"\",\"Middle\": \"\",\"Sell\": \"\",\"Star\": null},{\"Country\": \"China\",\"NumCode\": \"156\",\"AlphaCode\": \"CNY\",\"Units\": \"1\",\"Buy\": \"0.275802\",\"Middle\": \"0.276493\",\"Sell\": \"0.277184\",\"Star\": null},{\"Country\": \"Serbia\",\"NumCode\": \"941\",\"AlphaCode\": \"RSD\",\"Units\": \"100\",\"Buy\": \"1.660943\",\"Middle\": \"1.665106\",\"Sell\": \"1.669269\",\"Star\": null},{\"Country\": \"IMF\",\"NumCode\": \"960\",\"AlphaCode\": \"XDR\",\"Units\": \"1\",\"Buy\": \"\",\"Middle\": \"2.482868\",\"Sell\": \"\",\"Star\": null}],\"Date\": \"2022-06-28T00:00:00\",\"Comments\": [],\"Number\": 125}"
            .as_bytes();
        let timestamp: u64 = 1656374400;
        let extracted_rates = bosnia.extract_rate(query_response, timestamp);

        assert!(matches!(extracted_rates, Ok(rates) if rates["eur"] == 10_571));
    }

    /// The function tests if the [BankOfIsrael] struct returns the correct forex rate.
    #[test]
    fn extract_rate_from_israel() {
        let israel = BankOfIsrael;
        let query_response = "<?xml version=\"1.0\" encoding=\"utf-8\" standalone=\"yes\"?><CURRENCIES>  <LAST_UPDATE>2022-06-28</LAST_UPDATE>  <CURRENCY>    <NAME>Dollar</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>USD</CURRENCYCODE>    <COUNTRY>USA</COUNTRY>    <RATE>3.436</RATE>    <CHANGE>1.148</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Pound</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>GBP</CURRENCYCODE>    <COUNTRY>Great Britain</COUNTRY>    <RATE>4.2072</RATE>    <CHANGE>0.824</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Yen</NAME>    <UNIT>100</UNIT>    <CURRENCYCODE>JPY</CURRENCYCODE>    <COUNTRY>Japan</COUNTRY>    <RATE>2.5239</RATE>    <CHANGE>0.45</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Euro</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>EUR</CURRENCYCODE>    <COUNTRY>EMU</COUNTRY>    <RATE>3.6350</RATE>    <CHANGE>1.096</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Dollar</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>AUD</CURRENCYCODE>    <COUNTRY>Australia</COUNTRY>    <RATE>2.3866</RATE>    <CHANGE>1.307</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Dollar</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>CAD</CURRENCYCODE>    <COUNTRY>Canada</COUNTRY>    <RATE>2.6774</RATE>    <CHANGE>1.621</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>krone</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>DKK</CURRENCYCODE>    <COUNTRY>Denmark</COUNTRY>    <RATE>0.4885</RATE>    <CHANGE>1.097</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Krone</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>NOK</CURRENCYCODE>    <COUNTRY>Norway</COUNTRY>    <RATE>0.3508</RATE>    <CHANGE>1.622</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Rand</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>ZAR</CURRENCYCODE>    <COUNTRY>South Africa</COUNTRY>    <RATE>0.2155</RATE>    <CHANGE>0.701</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Krona</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>SEK</CURRENCYCODE>    <COUNTRY>Sweden</COUNTRY>    <RATE>0.3413</RATE>    <CHANGE>1.276</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Franc</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>CHF</CURRENCYCODE>    <COUNTRY>Switzerland</COUNTRY>    <RATE>3.5964</RATE>    <CHANGE>1.416</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Dinar</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>JOD</CURRENCYCODE>    <COUNTRY>Jordan</COUNTRY>    <RATE>4.8468</RATE>    <CHANGE>1.163</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Pound</NAME>    <UNIT>10</UNIT>    <CURRENCYCODE>LBP</CURRENCYCODE>    <COUNTRY>Lebanon</COUNTRY>    <RATE>0.0227</RATE>    <CHANGE>0.889</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Pound</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>EGP</CURRENCYCODE>    <COUNTRY>Egypt</COUNTRY>    <RATE>0.1830</RATE>    <CHANGE>1.049</CHANGE>  </CURRENCY></CURRENCIES>"
            .as_bytes();
        let timestamp: u64 = 1656374400;
        let extracted_rates = israel.extract_rate(query_response, timestamp);

        assert!(matches!(extracted_rates, Ok(rates) if rates["eur"] == 10_579));
    }
}
