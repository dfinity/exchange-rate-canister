use chrono::naive::NaiveDateTime;
use ic_cdk::export::candid::{
    decode_args, decode_one, encode_args, encode_one, CandidType, Deserialize, Error as CandidError,
};
use jaq_core::Val;
use std::cmp::min;
use std::str::FromStr;
use std::{collections::HashMap, convert::TryInto};

use crate::candid::ExchangeRateError;
use crate::{jq, median};
use crate::{ExtractError, USD};

/// The IMF SDR weights used to compute the XDR rate.
pub(crate) const USD_XDR_WEIGHT_PER_MILLION: u64 = 582_520;
pub(crate) const EUR_XDR_WEIGHT_PER_MILLION: u64 = 386_710;
pub(crate) const CNY_XDR_WEIGHT_PER_MILLION: u64 = 1_017_400;
pub(crate) const JPY_XDR_WEIGHT_PER_MILLION: u64 = 11_900_000;
pub(crate) const GBP_XDR_WEIGHT_PER_MILLION: u64 = 85_946;

/// The CMC uses a computed XDR (CXDR) rate based on the IMF SDR weights.
pub(crate) const COMPUTED_XDR_SYMBOL: &str = "CXDR";

/// A forex rate representation, includes the rate and the number of sources used to compute it.
#[derive(CandidType, Deserialize, Clone, Copy, Debug)]
pub struct ForexRate {
    pub rate: u64,
    pub num_sources: u64,
}

/// A map of multiple forex rates with one source per forex. The key is the forex symbol and the value is the corresponding rate.
pub type ForexRateMap = HashMap<String, u64>;

/// A map of multiple forex rates with possibly multiple sources per forex. The key is the forex symbol and the value is the corresponding rate and the number of sources used to compute it.
pub type ForexMultiRateMap = HashMap<String, ForexRate>;

/// The forex rate storage struct. Stores a map of <timestamp, [ForexMultiRateMap]>.
#[allow(dead_code)]
#[derive(CandidType, Deserialize, Clone, Debug)]
pub struct ForexRateStore {
    rates: HashMap<u64, ForexMultiRateMap>,
}

/// A forex rate collector. Allows the collection of multiple rates from different sources, and outputs the
/// aggregated [ForexMultiRateMap] to be stored.
#[allow(dead_code)]
#[derive(Clone, Debug)]
struct ForexRatesCollector {
    rates: HashMap<String, Vec<u64>>,
    timestamp: u64,
}

const SECONDS_PER_DAY: u64 = 60 * 60 * 24;

/// This macro generates the necessary boilerplate when adding a forex data source to this module.
macro_rules! forex {
    ($($name:ident),*) => {
        /// Enum that contains all of the possible forex sources.
        #[derive(PartialEq)]
        pub enum Forex {
            $(
                #[allow(missing_docs)]
                #[allow(dead_code)]
                $name($name),
            )*
        }

        $(
            #[derive(PartialEq)]
            pub struct $name;
        )*

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

            /// Retrieves the position of the exchange in the FOREX_SOURCES array.
            #[allow(dead_code)]
            pub fn get_id(&self) -> usize {
                FOREX_SOURCES.iter().position(|e| e == self).expect("should contain the forex")
            }

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

            /// This method is used to transform the HTTP response body based on the given payload.
            /// The payload contains additional context for the specific forex to extract the rate.
            pub fn transform_http_response_body(&self, body: &[u8], payload: &[u8]) -> Result<Vec<u8>, TransformHttpResponseError> {
                match self {
                    $(Forex::$name(forex) => forex.transform_http_response_body(body, payload)),*,
                }
            }

            /// This method encodes the context for the given exchange based on the provided arguments.
            pub fn encode_context(&self, args: &ForexContextArgs) -> Result<Vec<u8>, CandidError> {
                let id = self.get_id();
                let payload = match self {
                    $(Forex::$name(forex) => forex.encode_payload(args)?),*,
                };
                encode_one(ForexContext {
                    id,
                    payload
                })
            }

            /// A wrapper function to extract out the context provided in the transform's arguments.
            pub fn decode_context(bytes: &[u8]) -> Result<ForexContext, CandidError> {
                decode_one(bytes)
            }

            /// A wrapper to decode the response from the transform function.
            pub fn decode_response(bytes: &[u8]) -> Result<ForexRateMap, CandidError> {
                decode_one(bytes)
            }
        }
    }

}

forex! { MonetaryAuthorityOfSingapore, CentralBankOfMyanmar, CentralBankOfBosniaHerzegovina, BankOfIsrael, EuropeanCentralBank, BankOfCanada, CentralBankOfUzbekistan }

pub struct ForexContextArgs {
    pub timestamp: u64,
}

#[derive(CandidType, Deserialize)]
pub struct ForexContext {
    pub id: usize,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone)]
pub enum GetForexRateError {
    InvalidTimestamp(u64),
    CouldNotFindBaseAsset(u64, String),
    CouldNotFindQuoteAsset(u64, String),
    CouldNotFindAssets(u64, String, String),
}

impl From<GetForexRateError> for ExchangeRateError {
    fn from(error: GetForexRateError) -> Self {
        match error {
            GetForexRateError::InvalidTimestamp(_) => ExchangeRateError::ForexInvalidTimestamp,
            GetForexRateError::CouldNotFindBaseAsset(_, _) => {
                ExchangeRateError::ForexBaseAssetNotFound
            }
            GetForexRateError::CouldNotFindQuoteAsset(_, _) => {
                ExchangeRateError::ForexQuoteAssetNotFound
            }
            GetForexRateError::CouldNotFindAssets(_, _, _) => {
                ExchangeRateError::ForexAssetsNotFound
            }
        }
    }
}

impl core::fmt::Display for GetForexRateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GetForexRateError::InvalidTimestamp(timestamp) => {
                write!(f, "No forex rates found for date {}", timestamp)
            }
            GetForexRateError::CouldNotFindBaseAsset(timestamp, asset)
            | GetForexRateError::CouldNotFindQuoteAsset(timestamp, asset) => {
                write!(f, "No rate found for {} for date {}", asset, timestamp)
            }
            GetForexRateError::CouldNotFindAssets(timestamp, base_asset, quote_asset) => {
                write!(
                    f,
                    "No forex rate for {} or {} for date {}",
                    base_asset, quote_asset, timestamp
                )
            }
        }
    }
}

#[allow(dead_code)]
impl ForexRateStore {
    pub fn new() -> Self {
        Self {
            rates: HashMap::new(),
        }
    }

    /// Returns the exchange rate for the given two forex assets and a given timestamp, or None if a rate cannot be found.
    pub fn get(
        &self,
        timestamp: u64,
        base_asset: &str,
        quote_asset: &str,
    ) -> Result<ForexRate, GetForexRateError> {
        // Normalize timestamp to the beginning of the day
        let timestamp = (timestamp / SECONDS_PER_DAY) * SECONDS_PER_DAY;

        let base_asset = base_asset.to_uppercase();
        let quote_asset = quote_asset.to_uppercase();
        if base_asset == quote_asset {
            return Ok(ForexRate {
                rate: 10_000,
                num_sources: 0,
            });
        }

        if let Some(rates_for_timestamp) = self.rates.get(&timestamp) {
            let base = rates_for_timestamp.get(&base_asset);
            let quote = rates_for_timestamp.get(&quote_asset);

            match (base, quote) {
                (Some(base_rate), Some(quote_rate)) => Ok(ForexRate {
                    rate: (10_000 * base_rate.rate) / quote_rate.rate,
                    num_sources: std::cmp::min(base_rate.num_sources, quote_rate.num_sources),
                }),
                (Some(base_rate), None) => {
                    // If the quote asset is USD, it should not be present in the map and the base rate already uses USD as the quote asset.
                    if quote_asset == USD {
                        Ok(*base_rate)
                    } else {
                        Err(GetForexRateError::CouldNotFindQuoteAsset(
                            timestamp,
                            quote_asset.to_string(),
                        ))
                    }
                }
                (None, Some(_)) => Err(GetForexRateError::CouldNotFindBaseAsset(
                    timestamp,
                    base_asset.to_string(),
                )),
                (None, None) => {
                    if quote_asset == USD {
                        Err(GetForexRateError::CouldNotFindBaseAsset(
                            timestamp,
                            base_asset.to_string(),
                        ))
                    } else {
                        Err(GetForexRateError::CouldNotFindAssets(
                            timestamp,
                            base_asset.to_string(),
                            quote_asset.to_string(),
                        ))
                    }
                }
            }
        } else {
            Err(GetForexRateError::InvalidTimestamp(timestamp))
        }
    }

    /// Puts or updates rates for a given timestamp. If rates already exist for the given timestamp, only rates for which a new rate with higher number of sources are replaced.
    pub fn put(&mut self, timestamp: u64, rates: ForexMultiRateMap) {
        // Normalize timestamp to the beginning of the day
        let timestamp = (timestamp / SECONDS_PER_DAY) * SECONDS_PER_DAY;

        if let Some(ratesmap) = self.rates.get_mut(&timestamp) {
            // Update only the rates where the number of sources is higher.
            rates.into_iter().for_each(|(symbol, rate)| {
                // We should never insert rates for USD.
                if symbol != USD {
                    ratesmap
                        .entry(symbol)
                        .and_modify(|v| {
                            if v.num_sources < rate.num_sources {
                                *v = rate
                            }
                        })
                        .or_insert(rate);
                }
            });
        } else {
            // Insert the new rates.
            self.rates.insert(timestamp, rates);
        }
    }
}

#[allow(dead_code)]
impl ForexRatesCollector {
    fn new(timestamp: u64) -> Self {
        Self {
            rates: HashMap::new(),
            timestamp,
        }
    }

    /// Updates the collected rates with a new set of rates. The provided timestamp must match the collector's existing timestamp. The function returns true if the collector has been updated, or false if the timestamps did not match.
    fn update(&mut self, timestamp: u64, rates: ForexRateMap) -> bool {
        if timestamp != self.timestamp {
            false
        } else {
            rates.into_iter().for_each(|(symbol, rate)| {
                self.rates
                    .entry(if symbol == "SDR" {
                        "XDR".to_string()
                    } else {
                        symbol
                    })
                    .and_modify(|v| v.push(rate))
                    .or_insert_with(|| vec![rate]);
            });
            true
        }
    }

    /// Extracts the up-to-date median rates based on all existing rates.
    fn get_rates_map(&self) -> ForexMultiRateMap {
        let mut rates: ForexMultiRateMap = self
            .rates
            .iter()
            .map(|(k, v)| {
                (
                    k.to_string(),
                    ForexRate {
                        rate: crate::utils::median(v.as_slice()),
                        num_sources: v.len() as u64,
                    },
                )
            })
            .collect();
        if let Some(rate) = self.get_computed_xdr_rate() {
            rates.insert(COMPUTED_XDR_SYMBOL.to_string(), rate);
        }
        rates
    }

    /// Computes and returns the XDR/USD rate based on the weights specified by the IMF.
    fn get_computed_xdr_rate(&self) -> Option<ForexRate> {
        let eur_rates_option = self.rates.get("EUR");
        let cny_rates_option = self.rates.get("CNY");
        let jpy_rates_option = self.rates.get("JPY");
        let gbp_rates_option = self.rates.get("GBP");
        if let (Some(eur_rates), Some(cny_rates), Some(jpy_rates), Some(gbp_rates)) = (
            eur_rates_option,
            cny_rates_option,
            jpy_rates_option,
            gbp_rates_option,
        ) {
            let eur_rate = median(eur_rates);
            let cny_rate = median(cny_rates);
            let jpy_rate = median(jpy_rates);
            let gbp_rate = median(gbp_rates);

            // The factor 10_000 is the scaled USD/USD rate, i.e., the rate 1.00 permyriad.
            let xdr_rate = (USD_XDR_WEIGHT_PER_MILLION * 10_000
                + EUR_XDR_WEIGHT_PER_MILLION * eur_rate
                + CNY_XDR_WEIGHT_PER_MILLION * cny_rate
                + JPY_XDR_WEIGHT_PER_MILLION * jpy_rate
                + GBP_XDR_WEIGHT_PER_MILLION * gbp_rate)
                / 1_000_000;
            let xdr_num_sources = min(
                min(min(eur_rates.len(), cny_rates.len()), jpy_rates.len()),
                gbp_rates.len(),
            );

            Some(ForexRate {
                rate: xdr_rate as u64,
                num_sources: xdr_num_sources as u64,
            })
        } else {
            None
        }
    }

    /// Returns the timestamp corresponding to this collector.
    fn get_timestamp(&self) -> u64 {
        self.timestamp
    }
}

pub fn collect_rates(timestamp: u64, maps: Vec<ForexRateMap>) -> ForexMultiRateMap {
    let mut collector = ForexRatesCollector::new(timestamp);
    for map in maps {
        collector.update(timestamp, map);
    }
    collector.get_rates_map()
}

/// The base URL may contain the following placeholders:
/// `DATE`: This string must be replaced with the timestamp string as provided by `format_timestamp`.
const DATE: &str = "DATE";

/// The possible errors that can occur when calling an exchange.
#[derive(Debug)]
pub enum TransformHttpResponseError {
    /// Error that occurs when extracting the rate from the response.
    Extract(ExtractError),
    /// Error used when there is a failure encoding or decoding candid.
    Candid(CandidError),
}

impl core::fmt::Display for TransformHttpResponseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransformHttpResponseError::Extract(error) => {
                write!(f, "Failed to extract rate: {error}")
            }
            TransformHttpResponseError::Candid(error) => {
                write!(f, "Failed to encode/decode: {error}")
            }
        }
    }
}

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
        match values.get(USD) {
            Some(usd_value) => Ok(values
                .iter()
                .map(|(symbol, value)| (symbol.to_string(), (10_000 * value) / usd_value))
                .collect()),
            None => Err(ExtractError::RateNotFound {
                filter: "No USD rate".to_string(),
            }),
        }
    }

    /// Indicates if the exchange supports IPv6.
    fn supports_ipv6(&self) -> bool {
        false
    }

    /// Transforms the response body by using the provided payload. The payload contains arguments
    /// the forex needs in order to extract the rate.
    fn transform_http_response_body(
        &self,
        body: &[u8],
        payload: &[u8],
    ) -> Result<Vec<u8>, TransformHttpResponseError> {
        let timestamp = decode_args::<(u64,)>(payload)
            .map_err(TransformHttpResponseError::Candid)?
            .0;
        let forex_rate_map = self
            .extract_rate(body, timestamp)
            .map_err(TransformHttpResponseError::Extract)?;
        encode_one(forex_rate_map).map_err(TransformHttpResponseError::Candid)
    }

    /// Encodes the context given the particular arguments.
    fn encode_payload(&self, args: &ForexContextArgs) -> Result<Vec<u8>, CandidError> {
        encode_args((args.timestamp,))
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
                                                            symbol.to_uppercase(),
                                                            (rate * 100.0) as u64,
                                                        ))
                                                    } else {
                                                        Some((
                                                            symbol.to_uppercase(),
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
                values.insert("SGD".to_string(), 10_000);
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

    fn supports_ipv6(&self) -> bool {
        true
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
                                    Some((key.to_string().to_uppercase(), (rate * 10_000.0) as u64))
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

    fn supports_ipv6(&self) -> bool {
        true
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
                                        asset.to_uppercase(),
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

    fn supports_ipv6(&self) -> bool {
        true
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

        let data: XmlBankOfIsraelCurrencies =
            serde_xml_rs::from_reader(&bytes[3..]).map_err(|_| {
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
                        item.currencycode.to_uppercase(),
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

    fn supports_ipv6(&self) -> bool {
        true
    }
}

#[derive(Deserialize, Debug)]
enum XmlEcbOptions {
    #[serde(rename = "subject")]
    Subject(String),
    #[serde(rename = "Sender")]
    Sender(XmlEcbSender),
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
        let timestamp = (timestamp / SECONDS_PER_DAY) * SECONDS_PER_DAY;

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
            .unwrap_or_else(|_| NaiveDateTime::from_timestamp(0, 0))
            .timestamp() as u64;

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
                            ((1.0 / cube.rate) * 10_000.0) as u64,
                        )
                    })
                    .collect();
                values.insert("EUR".to_string(), 10_000);
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
}

/// Bank of Canada
impl IsForex for BankOfCanada {
    fn format_timestamp(&self, timestamp: u64) -> String {
        format!(
            "{}",
            NaiveDateTime::from_timestamp(timestamp.try_into().unwrap_or(0), 0).format("%Y-%m-%d")
        )
    }

    fn extract_rate(&self, bytes: &[u8], timestamp: u64) -> Result<ForexRateMap, ExtractError> {
        let timestamp = (timestamp / SECONDS_PER_DAY) * SECONDS_PER_DAY;

        let series = jq::extract(
            bytes,
            r#".seriesDetail | to_entries | map({ (.key): (.value.label) }) | add"#,
        )?;
        let values = jq::extract(
            bytes,
            r#".observations  | .[] | to_entries | map({ (.key): (.value | if type == "object" then .v else . end)}) | add"#,
        )?;
        let mut extracted_timestamp: u64 = 0;

        match (values, series) {
            (Val::Obj(values), Val::Obj(series)) => {
                let mut values_by_symbol = ForexRateMap::new();

                for (key, value) in values.iter() {
                    if let Val::Str(value) = value {
                        if key.to_string() == "d" {
                            // It is the date record
                            extracted_timestamp = NaiveDateTime::parse_from_str(
                                &(value.to_string() + " 00:00:00"),
                                "%Y-%m-%d %H:%M:%S",
                            )
                            .unwrap_or_else(|_| NaiveDateTime::from_timestamp(0, 0))
                            .timestamp() as u64;
                        } else {
                            // It is a series value - get the corresponding symbol and put into the map
                            if let Ok(val) = f64::from_str(&value.to_string()) {
                                if let Some(Val::Str(symbol_pair)) = series.get(key) {
                                    if let Some(symbol) = symbol_pair.to_string().split('/').next()
                                    {
                                        values_by_symbol
                                            .insert(symbol.to_uppercase(), (val * 10_000.0) as u64);
                                    }
                                }
                            };
                        }
                    }
                }
                if extracted_timestamp != timestamp {
                    Err(ExtractError::RateNotFound {
                        filter: "Invalid timestamp".to_string(),
                    })
                } else {
                    values_by_symbol.insert("CAD".to_string(), 10_000);
                    self.normalize_to_usd(&values_by_symbol)
                }
            }
            _ => Err(ExtractError::JsonDeserialize(
                "Not a valid object".to_string(),
            )),
        }
    }

    fn get_base_url(&self) -> &str {
        "https://www.bankofcanada.ca/valet/observations/group/FX_RATES_DAILY/json?start_date=DATE&end_date=DATE"
    }
}

/// Central Bank of Uzbekistan
impl IsForex for CentralBankOfUzbekistan {
    fn format_timestamp(&self, timestamp: u64) -> String {
        format!(
            "{}",
            NaiveDateTime::from_timestamp(timestamp.try_into().unwrap_or(0), 0).format("%Y-%m-%d")
        )
    }

    fn extract_rate(&self, bytes: &[u8], timestamp: u64) -> Result<ForexRateMap, ExtractError> {
        let timestamp = (timestamp / SECONDS_PER_DAY) * SECONDS_PER_DAY;

        let entries = jq::extract(bytes, ".")?;

        match entries {
            Val::Arr(values) => {
                let mut extracted_date: String = String::new();

                let rates: ForexRateMap = values
                    .iter()
                    .filter_map(|entry| {
                        if let Val::Obj(obj) = entry {
                            match (
                                obj.get(&"Ccy".to_string()),
                                obj.get(&"Rate".to_string()),
                                obj.get(&"Date".to_string()),
                            ) {
                                (
                                    Some(Val::Str(symbol)),
                                    Some(Val::Str(rate)),
                                    Some(Val::Str(datestr)),
                                ) => {
                                    if let Ok(rate_numeric) = f64::from_str(rate) {
                                        extracted_date = datestr.to_string();
                                        Some((symbol.to_string(), (rate_numeric * 10_000.0) as u64))
                                    } else {
                                        None
                                    }
                                }
                                _ => None,
                            }
                        } else {
                            None
                        }
                    })
                    .collect();
                let extracted_timestamp = NaiveDateTime::parse_from_str(
                    &(extracted_date + " 00:00:00"),
                    "%d.%m.%Y %H:%M:%S",
                )
                .unwrap_or_else(|_| NaiveDateTime::from_timestamp(0, 0))
                .timestamp() as u64;
                if extracted_timestamp != timestamp {
                    Err(ExtractError::RateNotFound {
                        filter: "Invalid timestamp".to_string(),
                    })
                } else {
                    self.normalize_to_usd(&rates)
                }
            }
            _ => Err(ExtractError::JsonDeserialize(
                "Not a valid object".to_string(),
            )),
        }
    }

    fn get_base_url(&self) -> &str {
        "https://cbu.uz/ru/arkhiv-kursov-valyut/json/all/DATE/"
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
        let forex = Forex::EuropeanCentralBank(EuropeanCentralBank);
        assert_eq!(forex.to_string(), "EuropeanCentralBank");
        let forex = Forex::BankOfCanada(BankOfCanada);
        assert_eq!(forex.to_string(), "BankOfCanada");
        let forex = Forex::CentralBankOfUzbekistan(CentralBankOfUzbekistan);
        assert_eq!(forex.to_string(), "CentralBankOfUzbekistan");
    }

    /// The function tests if the macro correctly generates derive copies by
    /// verifying that the forex sources return the correct query string.
    #[test]
    fn query_string() {
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
        let ecb = EuropeanCentralBank;
        let query_string = ecb.get_url(timestamp);
        assert_eq!(
            query_string,
            "https://www.ecb.europa.eu/stats/eurofxref/eurofxref-daily.xml"
        );
        let canada = BankOfCanada;
        let query_string = canada.get_url(timestamp);
        assert_eq!(
            query_string,
            "https://www.bankofcanada.ca/valet/observations/group/FX_RATES_DAILY/json?start_date=2022-08-26&end_date=2022-08-26"
        );
        let uzbekistan = CentralBankOfUzbekistan;
        let query_string = uzbekistan.get_url(timestamp);
        assert_eq!(
            query_string,
            "https://cbu.uz/ru/arkhiv-kursov-valyut/json/all/2022-08-26/"
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

        assert!(matches!(extracted_rates, Ok(rates) if rates["EUR"] == 10_581));
    }

    /// The function tests if the [CentralBankOfMyanmar] struct returns the correct forex rate.
    #[test]
    fn extract_rate_from_myanmar() {
        let myanmar = CentralBankOfMyanmar;
        let query_response = "{\"info\": \"Central Bank of Myanmar\",\"description\": \"Official Website of Central Bank of Myanmar\",\"timestamp\": 1656374400,\"rates\": {\"USD\": \"1,850.0\",\"VND\": \"7.9543\",\"THB\": \"52.714\",\"SEK\": \"184.22\",\"LKR\": \"5.1676\",\"ZAR\": \"116.50\",\"RSD\": \"16.685\",\"SAR\": \"492.89\",\"RUB\": \"34.807\",\"PHP\": \"33.821\",\"PKR\": \"8.9830\",\"NOK\": \"189.43\",\"NZD\": \"1,165.9\",\"NPR\": \"14.680\",\"MYR\": \"420.74\",\"LAK\": \"12.419\",\"KWD\": \"6,033.2\",\"KRW\": \"144.02\",\"KES\": \"15.705\",\"ILS\": \"541.03\",\"IDR\": \"12.469\",\"INR\": \"23.480\",\"HKD\": \"235.76\",\"EGP\": \"98.509\",\"DKK\": \"263.36\",\"CZK\": \"79.239\",\"CNY\": \"276.72\",\"CAD\": \"1,442.7\",\"KHR\": \"45.488\",\"BND\": \"1,335.6\",\"BRL\": \"353.14\",\"BDT\": \"19.903\",\"AUD\": \"1,287.6\",\"JPY\": \"1,363.4\",\"CHF\": \"1,937.7\",\"GBP\": \"2,272.0\",\"SGD\": \"1,335.6\",\"EUR\": \"1,959.7\"}}"
            .as_bytes();
        let timestamp: u64 = 1656374400;
        let extracted_rates = myanmar.extract_rate(query_response, timestamp);

        assert!(matches!(extracted_rates, Ok(rates) if rates["EUR"] == 10_592));
    }

    /// The function tests if the [CentralBankOfBosniaHerzegovina] struct returns the correct forex rate.
    #[test]
    fn extract_rate_from_bosnia() {
        let bosnia = CentralBankOfBosniaHerzegovina;
        let query_response = "{\"CurrencyExchangeItems\": [{\"Country\": \"EMU\",\"NumCode\": \"978\",\"AlphaCode\": \"EUR\",\"Units\": \"1\",\"Buy\": \"1.955830\",\"Middle\": \"1.955830\",\"Sell\": \"1.955830\",\"Star\": null},{\"Country\": \"Australia\",\"NumCode\": \"036\",\"AlphaCode\": \"AUD\",\"Units\": \"1\",\"Buy\": \"1.276961\",\"Middle\": \"1.280161\",\"Sell\": \"1.283361\",\"Star\": null},{\"Country\": \"Canada\",\"NumCode\": \"124\",\"AlphaCode\": \"CAD\",\"Units\": \"1\",\"Buy\": \"1.430413\",\"Middle\": \"1.433998\",\"Sell\": \"1.437583\",\"Star\": null},{\"Country\": \"Croatia\",\"NumCode\": \"191\",\"AlphaCode\": \"HRK\",\"Units\": \"100\",\"Buy\": \"25.897554\",\"Middle\": \"25.962460\",\"Sell\": \"26.027366\",\"Star\": null},{\"Country\": \"Czech R\",\"NumCode\": \"203\",\"AlphaCode\": \"CZK\",\"Units\": \"1\",\"Buy\": \"0.078909\",\"Middle\": \"0.079107\",\"Sell\": \"0.079305\",\"Star\": null},{\"Country\": \"Dennmark\",\"NumCode\": \"208\",\"AlphaCode\": \"DKK\",\"Units\": \"1\",\"Buy\": \"0.262195\",\"Middle\": \"0.262852\",\"Sell\": \"0.263509\",\"Star\": null},{\"Country\": \"Hungary\",\"NumCode\": \"348\",\"AlphaCode\": \"HUF\",\"Units\": \"100\",\"Buy\": \"0.484562\",\"Middle\": \"0.485776\",\"Sell\": \"0.486990\",\"Star\": null},{\"Country\": \"Japan\",\"NumCode\": \"392\",\"AlphaCode\": \"JPY\",\"Units\": \"100\",\"Buy\": \"1.361913\",\"Middle\": \"1.365326\",\"Sell\": \"1.368739\",\"Star\": null},{\"Country\": \"Norway\",\"NumCode\": \"578\",\"AlphaCode\": \"NOK\",\"Units\": \"1\",\"Buy\": \"0.187446\",\"Middle\": \"0.187916\",\"Sell\": \"0.188386\",\"Star\": null},{\"Country\": \"Sweden\",\"NumCode\": \"752\",\"AlphaCode\": \"SEK\",\"Units\": \"1\",\"Buy\": \"0.182821\",\"Middle\": \"0.183279\",\"Sell\": \"0.183737\",\"Star\": null},{\"Country\": \"Switzerland\",\"NumCode\": \"756\",\"AlphaCode\": \"CHF\",\"Units\": \"1\",\"Buy\": \"1.923435\",\"Middle\": \"1.928256\",\"Sell\": \"1.933077\",\"Star\": null},{\"Country\": \"Turkey\",\"NumCode\": \"949\",\"AlphaCode\": \"TRY\",\"Units\": \"1\",\"Buy\": \"0.111613\",\"Middle\": \"0.111893\",\"Sell\": \"0.112173\",\"Star\": null},{\"Country\": \"G.Britain\",\"NumCode\": \"826\",\"AlphaCode\": \"GBP\",\"Units\": \"1\",\"Buy\": \"2.263272\",\"Middle\": \"2.268944\",\"Sell\": \"2.274616\",\"Star\": null},{\"Country\": \"USA\",\"NumCode\": \"840\",\"AlphaCode\": \"USD\",\"Units\": \"1\",\"Buy\": \"1.845384\",\"Middle\": \"1.850009\",\"Sell\": \"1.854634\",\"Star\": null},{\"Country\": \"Russia\",\"NumCode\": \"643\",\"AlphaCode\": \"RUB\",\"Units\": \"1\",\"Buy\": \"\",\"Middle\": \"\",\"Sell\": \"\",\"Star\": null},{\"Country\": \"China\",\"NumCode\": \"156\",\"AlphaCode\": \"CNY\",\"Units\": \"1\",\"Buy\": \"0.275802\",\"Middle\": \"0.276493\",\"Sell\": \"0.277184\",\"Star\": null},{\"Country\": \"Serbia\",\"NumCode\": \"941\",\"AlphaCode\": \"RSD\",\"Units\": \"100\",\"Buy\": \"1.660943\",\"Middle\": \"1.665106\",\"Sell\": \"1.669269\",\"Star\": null},{\"Country\": \"IMF\",\"NumCode\": \"960\",\"AlphaCode\": \"XDR\",\"Units\": \"1\",\"Buy\": \"\",\"Middle\": \"2.482868\",\"Sell\": \"\",\"Star\": null}],\"Date\": \"2022-06-28T00:00:00\",\"Comments\": [],\"Number\": 125}"
            .as_bytes();
        let timestamp: u64 = 1656374400;
        let extracted_rates = bosnia.extract_rate(query_response, timestamp);

        assert!(matches!(extracted_rates, Ok(rates) if rates["EUR"] == 10_571));
    }

    /// The function tests if the [BankOfIsrael] struct returns the correct forex rate.
    #[test]
    fn extract_rate_from_israel() {
        let israel = BankOfIsrael;
        let query_response = "\u{feff}<?xml version=\"1.0\" encoding=\"utf-8\" standalone=\"yes\"?><CURRENCIES>  <LAST_UPDATE>2022-06-28</LAST_UPDATE>  <CURRENCY>    <NAME>Dollar</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>USD</CURRENCYCODE>    <COUNTRY>USA</COUNTRY>    <RATE>3.436</RATE>    <CHANGE>1.148</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Pound</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>GBP</CURRENCYCODE>    <COUNTRY>Great Britain</COUNTRY>    <RATE>4.2072</RATE>    <CHANGE>0.824</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Yen</NAME>    <UNIT>100</UNIT>    <CURRENCYCODE>JPY</CURRENCYCODE>    <COUNTRY>Japan</COUNTRY>    <RATE>2.5239</RATE>    <CHANGE>0.45</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Euro</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>EUR</CURRENCYCODE>    <COUNTRY>EMU</COUNTRY>    <RATE>3.6350</RATE>    <CHANGE>1.096</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Dollar</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>AUD</CURRENCYCODE>    <COUNTRY>Australia</COUNTRY>    <RATE>2.3866</RATE>    <CHANGE>1.307</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Dollar</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>CAD</CURRENCYCODE>    <COUNTRY>Canada</COUNTRY>    <RATE>2.6774</RATE>    <CHANGE>1.621</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>krone</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>DKK</CURRENCYCODE>    <COUNTRY>Denmark</COUNTRY>    <RATE>0.4885</RATE>    <CHANGE>1.097</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Krone</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>NOK</CURRENCYCODE>    <COUNTRY>Norway</COUNTRY>    <RATE>0.3508</RATE>    <CHANGE>1.622</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Rand</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>ZAR</CURRENCYCODE>    <COUNTRY>South Africa</COUNTRY>    <RATE>0.2155</RATE>    <CHANGE>0.701</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Krona</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>SEK</CURRENCYCODE>    <COUNTRY>Sweden</COUNTRY>    <RATE>0.3413</RATE>    <CHANGE>1.276</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Franc</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>CHF</CURRENCYCODE>    <COUNTRY>Switzerland</COUNTRY>    <RATE>3.5964</RATE>    <CHANGE>1.416</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Dinar</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>JOD</CURRENCYCODE>    <COUNTRY>Jordan</COUNTRY>    <RATE>4.8468</RATE>    <CHANGE>1.163</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Pound</NAME>    <UNIT>10</UNIT>    <CURRENCYCODE>LBP</CURRENCYCODE>    <COUNTRY>Lebanon</COUNTRY>    <RATE>0.0227</RATE>    <CHANGE>0.889</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Pound</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>EGP</CURRENCYCODE>    <COUNTRY>Egypt</COUNTRY>    <RATE>0.1830</RATE>    <CHANGE>1.049</CHANGE>  </CURRENCY></CURRENCIES>"
            .as_bytes();
        let timestamp: u64 = 1656374400;
        let extracted_rates = israel.extract_rate(query_response, timestamp);

        assert!(matches!(extracted_rates, Ok(rates) if rates["EUR"] == 10_579));
    }

    /// The function tests if the [EuropeanCentralBank] struct returns the correct forex rate.
    #[test]
    fn extract_rate_from_ecb() {
        let ecb = EuropeanCentralBank;
        let query_response = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><gesmes:Envelope xmlns:gesmes=\"http://www.gesmes.org/xml/2002-08-01\" xmlns=\"http://www.ecb.int/vocabulary/2002-08-01/eurofxref\">	<gesmes:subject>Reference rates</gesmes:subject>	<gesmes:Sender>		<gesmes:name>European Central Bank</gesmes:name>	</gesmes:Sender>	<Cube>		<Cube time='2022-10-03'>			<Cube currency='USD' rate='0.9764'/>			<Cube currency='JPY' rate='141.49'/>			<Cube currency='BGN' rate='1.9558'/>			<Cube currency='CZK' rate='24.527'/>			<Cube currency='DKK' rate='7.4366'/>			<Cube currency='GBP' rate='0.87070'/>			<Cube currency='HUF' rate='424.86'/>			<Cube currency='PLN' rate='4.8320'/>			<Cube currency='RON' rate='4.9479'/>			<Cube currency='SEK' rate='10.8743'/>			<Cube currency='CHF' rate='0.9658'/>			<Cube currency='ISK' rate='141.70'/>			<Cube currency='NOK' rate='10.5655'/>			<Cube currency='HRK' rate='7.5275'/>			<Cube currency='TRY' rate='18.1240'/>			<Cube currency='AUD' rate='1.5128'/>			<Cube currency='BRL' rate='5.1780'/>			<Cube currency='CAD' rate='1.3412'/>			<Cube currency='CNY' rate='6.9481'/>			<Cube currency='HKD' rate='7.6647'/>			<Cube currency='IDR' rate='14969.79'/>			<Cube currency='ILS' rate='3.4980'/>			<Cube currency='INR' rate='79.8980'/>			<Cube currency='KRW' rate='1408.25'/>			<Cube currency='MXN' rate='19.6040'/>			<Cube currency='MYR' rate='4.5383'/>			<Cube currency='NZD' rate='1.7263'/>			<Cube currency='PHP' rate='57.599'/>			<Cube currency='SGD' rate='1.4015'/>			<Cube currency='THB' rate='37.181'/>			<Cube currency='ZAR' rate='17.5871'/>		</Cube>	</Cube></gesmes:Envelope>"
            .as_bytes();
        let timestamp: u64 = 1664755200;
        let extracted_rates = ecb.extract_rate(query_response, timestamp);

        assert!(matches!(extracted_rates, Ok(rates) if rates["EUR"] == 9_764));
    }

    /// The function tests if the [BankOfCanada] struct returns the correct forex rate.
    #[test]
    fn extract_rate_from_canada() {
        let canada = BankOfCanada;
        let query_response = "{    \"groupDetail\": {        \"label\": \"Daily exchange rates\",        \"description\": \"Daily average exchange rates - published once each business day by 16:30 ET. All Bank of Canada exchange rates are indicative rates only.\",        \"link\": null    },    \"terms\": {        \"url\": \"https://www.bankofcanada.ca/terms/\"    },    \"seriesDetail\": {        \"FXAUDCAD\": {            \"label\": \"AUD/CAD\",            \"description\": \"Australian dollar to Canadian dollar daily exchange rate\",            \"dimension\": {                \"key\": \"d\",                \"name\": \"date\"            }        },        \"FXBRLCAD\": {            \"label\": \"BRL/CAD\",            \"description\": \"Brazilian real to Canadian dollar daily exchange rate\",            \"dimension\": {                \"key\": \"d\",                \"name\": \"date\"            }        },        \"FXCNYCAD\": {            \"label\": \"CNY/CAD\",            \"description\": \"Chinese renminbi to Canadian dollar daily exchange rate\",            \"dimension\": {                \"key\": \"d\",                \"name\": \"date\"            }        },        \"FXEURCAD\": {            \"label\": \"EUR/CAD\",            \"description\": \"European euro to Canadian dollar daily exchange rate\",            \"dimension\": {                \"key\": \"d\",                \"name\": \"date\"            }        },        \"FXHKDCAD\": {            \"label\": \"HKD/CAD\",            \"description\": \"Hong Kong dollar to Canadian dollar daily exchange rate\",            \"dimension\": {                \"key\": \"d\",                \"name\": \"date\"            }        },        \"FXINRCAD\": {            \"label\": \"INR/CAD\",            \"description\": \"Indian rupee to Canadian dollar daily exchange rate\",            \"dimension\": {                \"key\": \"d\",                \"name\": \"date\"            }        },        \"FXIDRCAD\": {            \"label\": \"IDR/CAD\",            \"description\": \"Indonesian rupiah to Canadian dollar daily exchange rate\",            \"dimension\": {                \"key\": \"d\",                \"name\": \"date\"            }        },        \"FXJPYCAD\": {            \"label\": \"JPY/CAD\",            \"description\": \"Japanese yen to Canadian dollar daily exchange rate\",            \"dimension\": {                \"key\": \"d\",                \"name\": \"date\"            }        },        \"FXMYRCAD\": {            \"label\": \"MYR/CAD\",            \"description\": \"Malaysian ringgit to Canadian dollar daily exchange rate\",            \"dimension\": {                \"key\": \"d\",                \"name\": \"date\"            }        },        \"FXMXNCAD\": {            \"label\": \"MXN/CAD\",            \"description\": \"Mexican peso to Canadian dollar daily exchange rate\",            \"dimension\": {                \"key\": \"d\",                \"name\": \"date\"            }        },        \"FXNZDCAD\": {            \"label\": \"NZD/CAD\",            \"description\": \"New Zealand dollar to Canadian dollar daily exchange rate\",            \"dimension\": {                \"key\": \"d\",                \"name\": \"date\"            }        },        \"FXNOKCAD\": {            \"label\": \"NOK/CAD\",            \"description\": \"Norwegian krone to Canadian dollar daily exchange rate\",            \"dimension\": {                \"key\": \"d\",                \"name\": \"date\"            }        },        \"FXPENCAD\": {            \"label\": \"PEN/CAD\",            \"description\": \"Peruvian new sol to Canadian dollar daily exchange rate\",            \"dimension\": {                \"key\": \"d\",                \"name\": \"date\"            }        },        \"FXRUBCAD\": {            \"label\": \"RUB/CAD\",            \"description\": \"Russian ruble to Canadian dollar daily exchange rate\",            \"dimension\": {                \"key\": \"d\",                \"name\": \"date\"            }        },        \"FXSARCAD\": {            \"label\": \"SAR/CAD\",            \"description\": \"Saudi riyal to Canadian dollar daily exchange rate\",            \"dimension\": {                \"key\": \"d\",                \"name\": \"date\"            }        },        \"FXSGDCAD\": {            \"label\": \"SGD/CAD\",            \"description\": \"Singapore dollar to Canadian dollar daily exchange rate\",            \"dimension\": {                \"key\": \"d\",                \"name\": \"date\"            }        },        \"FXZARCAD\": {            \"label\": \"ZAR/CAD\",            \"description\": \"South African rand to Canadian dollar daily exchange rate\",            \"dimension\": {                \"key\": \"d\",                \"name\": \"date\"            }        },        \"FXKRWCAD\": {            \"label\": \"KRW/CAD\",            \"description\": \"South Korean won to Canadian dollar daily exchange rate\",            \"dimension\": {                \"key\": \"d\",                \"name\": \"date\"            }        },        \"FXSEKCAD\": {            \"label\": \"SEK/CAD\",            \"description\": \"Swedish krona to Canadian dollar daily exchange rate\",            \"dimension\": {                \"key\": \"d\",                \"name\": \"date\"            }        },        \"FXCHFCAD\": {            \"label\": \"CHF/CAD\",            \"description\": \"Swiss franc to Canadian dollar daily exchange rate\",            \"dimension\": {                \"key\": \"d\",                \"name\": \"date\"            }        },        \"FXTWDCAD\": {            \"label\": \"TWD/CAD\",            \"description\": \"Taiwanese dollar to Canadian dollar daily exchange rate\",            \"dimension\": {                \"key\": \"d\",                \"name\": \"date\"            }        },        \"FXTHBCAD\": {            \"label\": \"THB/CAD\",            \"description\": \"Thai baht to Canadian dollar daily exchange rate\",            \"dimension\": {                \"key\": \"d\",                \"name\": \"date\"            }        },        \"FXTRYCAD\": {            \"label\": \"TRY/CAD\",            \"description\": \"Turkish lira to Canadian dollar daily exchange rate\",            \"dimension\": {                \"key\": \"d\",                \"name\": \"date\"            }        },        \"FXGBPCAD\": {            \"label\": \"GBP/CAD\",            \"description\": \"UK pound sterling to Canadian dollar daily exchange rate\",            \"dimension\": {                \"key\": \"d\",                \"name\": \"date\"            }        },        \"FXUSDCAD\": {            \"label\": \"USD/CAD\",            \"description\": \"US dollar to Canadian dollar daily exchange rate\",            \"dimension\": {                \"key\": \"d\",                \"name\": \"date\"            }        },        \"FXVNDCAD\": {            \"label\": \"VND/CAD\",            \"description\": \"Vietnamese dong to Canadian dollar daily exchange rate\",            \"dimension\": {                \"key\": \"d\",                \"name\": \"date\"            }        }    },    \"observations\": [        {            \"d\": \"2022-06-28\",            \"FXAUDCAD\": {                \"v\": \"0.8906\"            },            \"FXBRLCAD\": {                \"v\": \"0.2455\"            },            \"FXCNYCAD\": {                \"v\": \"0.1918\"            },            \"FXEURCAD\": {                \"v\": \"1.3545\"            },            \"FXHKDCAD\": {                \"v\": \"0.1639\"            },            \"FXINRCAD\": {                \"v\": \"0.01628\"            },            \"FXIDRCAD\": {                \"v\": \"0.000087\"            },            \"FXJPYCAD\": {                \"v\": \"0.009450\"            },            \"FXMXNCAD\": {                \"v\": \"0.06424\"            },            \"FXNZDCAD\": {                \"v\": \"0.8049\"            },            \"FXNOKCAD\": {                \"v\": \"0.1310\"            },            \"FXPENCAD\": {                \"v\": \"0.3401\"            },            \"FXRUBCAD\": {                \"v\": \"0.02403\"            },            \"FXSARCAD\": {                \"v\": \"0.3428\"            },            \"FXSGDCAD\": {                \"v\": \"0.9272\"            },            \"FXZARCAD\": {                \"v\": \"0.08017\"            },            \"FXKRWCAD\": {                \"v\": \"0.000997\"            },            \"FXSEKCAD\": {                \"v\": \"0.1270\"            },            \"FXCHFCAD\": {                \"v\": \"1.3444\"            },            \"FXTWDCAD\": {                \"v\": \"0.04328\"            },            \"FXTRYCAD\": {                \"v\": \"0.07730\"            },            \"FXGBPCAD\": {                \"v\": \"1.5696\"            },            \"FXUSDCAD\": {                \"v\": \"1.2864\"            }        }    ]}"
            .as_bytes();
        let timestamp: u64 = 1656374400;
        let extracted_rates = canada.extract_rate(query_response, timestamp);

        assert!(matches!(extracted_rates, Ok(rates) if rates["EUR"] == 10_529));
    }

    /// The function tests if the [CentralBankOfUzbekistan] struct returns the correct forex rate.
    #[test]
    fn extract_rate_from_uzbekistan() {
        let uzbekistan = CentralBankOfUzbekistan;
        let query_response = "[{\"id\": 69,\"Code\": \"840\",\"Ccy\": \"USD\",\"CcyNm_RU\": \"Доллар США\",\"CcyNm_UZ\": \"AQSH dollari\",\"CcyNm_UZC\": \"АҚШ доллари\",\"CcyNm_EN\": \"US Dollar\",\"Nominal\": \"1\",\"Rate\": \"10823.52\",\"Diff\": \"-16.38\",\"Date\": \"28.06.2022\"},{\"id\": 21,\"Code\": \"978\",\"Ccy\": \"EUR\",\"CcyNm_RU\": \"Евро\",\"CcyNm_UZ\": \"EVRO\",\"CcyNm_UZC\": \"EВРО\",\"CcyNm_EN\": \"Euro\",\"Nominal\": \"1\",\"Rate\": \"11439.38\",\"Diff\": \"0.03\",\"Date\": \"28.06.2022\"},{\"id\": 57,\"Code\": \"643\",\"Ccy\": \"RUB\",\"CcyNm_RU\": \"Российский рубль\",\"CcyNm_UZ\": \"Rossiya rubli\",\"CcyNm_UZC\": \"Россия рубли\",\"CcyNm_EN\": \"Russian Ruble\",\"Nominal\": \"1\",\"Rate\": \"203.16\",\"Diff\": \"0.17\",\"Date\": \"28.06.2022\"},{\"id\": 22,\"Code\": \"826\",\"Ccy\": \"GBP\",\"CcyNm_RU\": \"Фунт стерлингов\",\"CcyNm_UZ\": \"Angliya funt sterlingi\",\"CcyNm_UZC\": \"Англия фунт стерлинги\",\"CcyNm_EN\": \"Pound Sterling\",\"Nominal\": \"1\",\"Rate\": \"13290.20\",\"Diff\": \"-43.96\",\"Date\": \"28.06.2022\"},{\"id\": 33,\"Code\": \"392\",\"Ccy\": \"JPY\",\"CcyNm_RU\": \"Иена\",\"CcyNm_UZ\": \"Yaponiya iyenasi\",\"CcyNm_UZC\": \"Япония иенаси\",\"CcyNm_EN\": \"Japan Yen\",\"Nominal\": \"1\",\"Rate\": \"80.05\",\"Diff\": \"-0.23\",\"Date\": \"28.06.2022\"},{\"id\": 6,\"Code\": \"944\",\"Ccy\": \"AZN\",\"CcyNm_RU\": \"Азербайджанский манат\",\"CcyNm_UZ\": \"Ozarbayjon manati\",\"CcyNm_UZC\": \"Озарбайжон манати\",\"CcyNm_EN\": \"Azerbaijan Manat\",\"Nominal\": \"1\",\"Rate\": \"6370.52\",\"Diff\": \"-9.64\",\"Date\": \"28.06.2022\"},{\"id\": 7,\"Code\": \"050\",\"Ccy\": \"BDT\",\"CcyNm_RU\": \"Бангладешская така\",\"CcyNm_UZ\": \"Bangladesh takasi\",\"CcyNm_UZC\": \"Бангладеш такаси\",\"CcyNm_EN\": \"Bangladesh Taka\",\"Nominal\": \"1\",\"Rate\": \"116.44\",\"Diff\": \"-0.33\",\"Date\": \"28.06.2022\"},{\"id\": 8,\"Code\": \"975\",\"Ccy\": \"BGN\",\"CcyNm_RU\": \"Болгарский лев\",\"CcyNm_UZ\": \"Bolgariya levi\",\"CcyNm_UZC\": \"Болгария леви\",\"CcyNm_EN\": \"Bulgarian Lev\",\"Nominal\": \"1\",\"Rate\": \"5848.97\",\"Diff\": \"0.31\",\"Date\": \"28.06.2022\"},{\"id\": 9,\"Code\": \"048\",\"Ccy\": \"BHD\",\"CcyNm_RU\": \"Бахрейнский динар\",\"CcyNm_UZ\": \"Bahrayn dinori\",\"CcyNm_UZC\": \"Баҳрайн динори\",\"CcyNm_EN\": \"Bahraini Dinar\",\"Nominal\": \"1\",\"Rate\": \"28709.60\",\"Diff\": \"-43.45\",\"Date\": \"28.06.2022\"},{\"id\": 10,\"Code\": \"096\",\"Ccy\": \"BND\",\"CcyNm_RU\": \"Брунейский доллар\",\"CcyNm_UZ\": \"Bruney dollari\",\"CcyNm_UZC\": \"Бруней доллари\",\"CcyNm_EN\": \"Brunei Dollar\",\"Nominal\": \"1\",\"Rate\": \"7812.56\",\"Diff\": \"2.27\",\"Date\": \"28.06.2022\"},{\"id\": 11,\"Code\": \"986\",\"Ccy\": \"BRL\",\"CcyNm_RU\": \"Бразильский реал\",\"CcyNm_UZ\": \"Braziliya reali\",\"CcyNm_UZC\": \"Бразилия реали\",\"CcyNm_EN\": \"Brazilian Real\",\"Nominal\": \"1\",\"Rate\": \"2064.41\",\"Diff\": \"-4.55\",\"Date\": \"28.06.2022\"},{\"id\": 12,\"Code\": \"933\",\"Ccy\": \"BYN\",\"CcyNm_RU\": \"Белорусский рубль\",\"CcyNm_UZ\": \"Belorus rubli\",\"CcyNm_UZC\": \"Белорус рубли\",\"CcyNm_EN\": \"Belarusian Ruble\",\"Nominal\": \"1\",\"Rate\": \"3205.45\",\"Diff\": \"-4.85\",\"Date\": \"28.06.2022\"},{\"id\": 13,\"Code\": \"124\",\"Ccy\": \"CAD\",\"CcyNm_RU\": \"Канадский доллар\",\"CcyNm_UZ\": \"Kanada dollari\",\"CcyNm_UZC\": \"Канада доллари\",\"CcyNm_EN\": \"Canadian Dollar\",\"Nominal\": \"1\",\"Rate\": \"8394.88\",\"Diff\": \"31.4\",\"Date\": \"28.06.2022\"},{\"id\": 14,\"Code\": \"756\",\"Ccy\": \"CHF\",\"CcyNm_RU\": \"Швейцарский франк\",\"CcyNm_UZ\": \"Shveytsariya franki\",\"CcyNm_UZC\": \"Швейцария франки\",\"CcyNm_EN\": \"Swiss Franc\",\"Nominal\": \"1\",\"Rate\": \"11299.22\",\"Diff\": \"-23.01\",\"Date\": \"28.06.2022\"},{\"id\": 15,\"Code\": \"156\",\"Ccy\": \"CNY\",\"CcyNm_RU\": \"Юань ренминби\",\"CcyNm_UZ\": \"Xitoy yuani\",\"CcyNm_UZC\": \"Хитой юани\",\"CcyNm_EN\": \"Yuan Renminbi\",\"Nominal\": \"1\",\"Rate\": \"1617.96\",\"Diff\": \"-0.93\",\"Date\": \"28.06.2022\"},{\"id\": 16,\"Code\": \"192\",\"Ccy\": \"CUP\",\"CcyNm_RU\": \"Кубинское песо\",\"CcyNm_UZ\": \"Kuba pesosi\",\"CcyNm_UZC\": \"Куба песоси\",\"CcyNm_EN\": \"Cuban Peso\",\"Nominal\": \"1\",\"Rate\": \"450.98\",\"Diff\": \"-0.68\",\"Date\": \"28.06.2022\"},{\"id\": 17,\"Code\": \"203\",\"Ccy\": \"CZK\",\"CcyNm_RU\": \"Чешская крона\",\"CcyNm_UZ\": \"Chexiya kronasi\",\"CcyNm_UZC\": \"Чехия кронаси\",\"CcyNm_EN\": \"Czech Koruna\",\"Nominal\": \"1\",\"Rate\": \"462.44\",\"Diff\": \"0.2\",\"Date\": \"28.06.2022\"},{\"id\": 18,\"Code\": \"208\",\"Ccy\": \"DKK\",\"CcyNm_RU\": \"Датская крона\",\"CcyNm_UZ\": \"Daniya kronasi\",\"CcyNm_UZC\": \"Дания кронаси\",\"CcyNm_EN\": \"Danish Krone\",\"Nominal\": \"1\",\"Rate\": \"1537.34\",\"Diff\": \"-0.28\",\"Date\": \"28.06.2022\"},{\"id\": 19,\"Code\": \"012\",\"Ccy\": \"DZD\",\"CcyNm_RU\": \"Алжирский динар\",\"CcyNm_UZ\": \"Jazoir dinori\",\"CcyNm_UZC\": \"Жазоир динори\",\"CcyNm_EN\": \"Algerian Dinar\",\"Nominal\": \"1\",\"Rate\": \"74.21\",\"Diff\": \"-0.11\",\"Date\": \"28.06.2022\"},{\"id\": 20,\"Code\": \"818\",\"Ccy\": \"EGP\",\"CcyNm_RU\": \"Египетский фунт\",\"CcyNm_UZ\": \"Misr funti\",\"CcyNm_UZC\": \"Миср фунти\",\"CcyNm_EN\": \"Egyptian Pound\",\"Nominal\": \"1\",\"Rate\": \"576.35\",\"Diff\": \"-1.01\",\"Date\": \"28.06.2022\"}]"
            .as_bytes();
        let timestamp: u64 = 1656374400;
        let extracted_rates = uzbekistan.extract_rate(query_response, timestamp);

        assert!(matches!(extracted_rates, Ok(rates) if rates["EUR"] == 10_569));
    }

    /// Tests that the [ForexRatesCollector] struct correctly collects rates and computes the median over them.
    #[test]
    fn rate_collector_update_and_get() {
        // Create a collector, update three times, check median rates.
        let mut collector = ForexRatesCollector {
            rates: HashMap::new(),
            timestamp: 1234,
        };

        // Expect to fail due to unmatched timestamp.
        assert!(!collector.update(5678, ForexRateMap::new()));

        // Insert real values with the correct timestamp.
        let rates = vec![
            ("EUR".to_string(), 10_000),
            ("SGD".to_string(), 1_000),
            ("CHF".to_string(), 7_000),
        ]
        .into_iter()
        .collect();
        assert!(collector.update(1234, rates));
        let rates = vec![
            ("EUR".to_string(), 11_000),
            ("SGD".to_string(), 10_000),
            ("CHF".to_string(), 10_000),
        ]
        .into_iter()
        .collect();
        assert!(collector.update(1234, rates));
        let rates = vec![
            ("EUR".to_string(), 8_000),
            ("SGD".to_string(), 13_000),
            ("CHF".to_string(), 21_000),
        ]
        .into_iter()
        .collect();
        assert!(collector.update(1234, rates));

        let result = collector.get_rates_map();
        assert_eq!(result.len(), 3);
        result.values().for_each(|v| {
            assert_eq!(v.rate, 10_000);
            assert_eq!(v.num_sources, 3);
        });
    }

    /// Tests that the [ForexRatesStore] struct correctly updates rates for the same timestamp.
    #[test]
    fn rate_store_update() {
        // Create a store, update, check that only rates with more sources were updated.
        let mut store = ForexRateStore::new();
        store.put(
            1234,
            vec![
                (
                    "EUR".to_string(),
                    ForexRate {
                        rate: 8_000,
                        num_sources: 4,
                    },
                ),
                (
                    "SGD".to_string(),
                    ForexRate {
                        rate: 10_000,
                        num_sources: 5,
                    },
                ),
                (
                    "CHF".to_string(),
                    ForexRate {
                        rate: 21_000,
                        num_sources: 2,
                    },
                ),
            ]
            .into_iter()
            .collect(),
        );
        store.put(
            1234,
            vec![
                (
                    "EUR".to_string(),
                    ForexRate {
                        rate: 10_000,
                        num_sources: 5,
                    },
                ),
                (
                    "GBP".to_string(),
                    ForexRate {
                        rate: 10_000,
                        num_sources: 2,
                    },
                ),
                (
                    "CHF".to_string(),
                    ForexRate {
                        rate: 10_000,
                        num_sources: 5,
                    },
                ),
            ]
            .into_iter()
            .collect(),
        );
        assert!(matches!(
            store.get(1234, "EUR", USD),
            Ok(ForexRate {
                rate: 10_000,
                num_sources: 5
            })
        ));
        assert!(matches!(
            store.get(1234, "SGD", USD),
            Ok(ForexRate {
                rate: 10_000,
                num_sources: 5
            })
        ));
        assert!(matches!(
            store.get(1234, "CHF", USD),
            Ok(ForexRate {
                rate: 10_000,
                num_sources: 5
            })
        ));
        assert!(matches!(
            store.get(1234, "GBP", USD),
            Ok(ForexRate {
                rate: 10_000,
                num_sources: 2
            })
        ));
        assert!(matches!(
            store.get(1234, "CHF", "EUR"),
            Ok(ForexRate {
                rate: 10_000,
                num_sources: 5
            })
        ));

        let result = store.get(1234, "HKD", USD);
        assert!(
            matches!(result, Err(GetForexRateError::CouldNotFindBaseAsset(timestamp, ref asset)) if timestamp == (1234 / SECONDS_PER_DAY) * SECONDS_PER_DAY && asset == "HKD"),
            "Expected `Err(GetForexRateError::CouldNotFindBaseAsset)`, Got: {:?}",
            result
        );
    }

    #[test]
    fn rate_store_get_same_asset() {
        let store = ForexRateStore::new();
        let result = store.get(1234, USD, USD);
        assert!(
            matches!(result, Ok(forex_rate) if forex_rate.rate == 10_000 && forex_rate.num_sources == 0)
        );
        let result = store.get(1234, "CHF", "CHF");
        assert!(
            matches!(result, Ok(forex_rate) if forex_rate.rate == 10_000 && forex_rate.num_sources == 0)
        );
    }

    /// Test that SDR and XDR rates are reported as the same asset under the symbol "xdr"
    #[test]
    fn collector_sdr_xdr() {
        let mut collector = ForexRatesCollector {
            rates: HashMap::new(),
            timestamp: 1234,
        };

        let rates = vec![("SDR".to_string(), 10_000), ("XDR".to_string(), 7_000)]
            .into_iter()
            .collect();
        collector.update(1234, rates);

        let rates = vec![("SDR".to_string(), 11_000)].into_iter().collect();
        collector.update(1234, rates);

        let rates = vec![("SDR".to_string(), 10_500), ("XDR".to_string(), 9_000)]
            .into_iter()
            .collect();
        collector.update(1234, rates);

        assert!(matches!(
            collector.get_rates_map()["XDR"],
            ForexRate {
                rate: 10_000,
                num_sources: 5
            }
        ))
    }

    /// Tests that the [ForexRatesCollector] computes and adds the correct CXDR rate if
    /// all EUR/USD, CNY/USD, JPY/USD, and GBP/USD rates are available.
    #[test]
    fn verify_compute_xdr_rate() {
        let mut map: HashMap<String, Vec<u64>> = HashMap::new();
        map.insert("EUR".to_string(), vec![9795]);
        map.insert("CNY".to_string(), vec![1405]);
        map.insert("JPY".to_string(), vec![69]);
        map.insert("GBP".to_string(), vec![11212]);

        let collector = ForexRatesCollector {
            rates: map,
            timestamp: 0,
        };

        let rates_map = collector.get_rates_map();
        let cxdr_usd_rate = rates_map.get(COMPUTED_XDR_SYMBOL);

        // The expected CXDR/USD rate is
        // 0.58252+0.38671×0.9795+1.0174×0.1405+11.9×0.0069+0.085946×1.1212 = 1.2827
        let expected_rate = ForexRate {
            rate: 12_827,
            num_sources: 1,
        };
        assert!(matches!(cxdr_usd_rate, Some(rate) if rate.rate == expected_rate.rate));
    }

    /// Test transform_http_response_body to the correct set of bytes.
    #[test]
    fn encoding_transformed_http_response() {
        let forex = Forex::BankOfIsrael(BankOfIsrael);
        let body = "\u{feff}<?xml version=\"1.0\" encoding=\"utf-8\" standalone=\"yes\"?><CURRENCIES>  <LAST_UPDATE>2022-06-28</LAST_UPDATE>  <CURRENCY>    <NAME>Dollar</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>USD</CURRENCYCODE>    <COUNTRY>USA</COUNTRY>    <RATE>3.436</RATE>    <CHANGE>1.148</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Pound</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>GBP</CURRENCYCODE>    <COUNTRY>Great Britain</COUNTRY>    <RATE>4.2072</RATE>    <CHANGE>0.824</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Yen</NAME>    <UNIT>100</UNIT>    <CURRENCYCODE>JPY</CURRENCYCODE>    <COUNTRY>Japan</COUNTRY>    <RATE>2.5239</RATE>    <CHANGE>0.45</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Euro</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>EUR</CURRENCYCODE>    <COUNTRY>EMU</COUNTRY>    <RATE>3.6350</RATE>    <CHANGE>1.096</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Dollar</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>AUD</CURRENCYCODE>    <COUNTRY>Australia</COUNTRY>    <RATE>2.3866</RATE>    <CHANGE>1.307</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Dollar</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>CAD</CURRENCYCODE>    <COUNTRY>Canada</COUNTRY>    <RATE>2.6774</RATE>    <CHANGE>1.621</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>krone</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>DKK</CURRENCYCODE>    <COUNTRY>Denmark</COUNTRY>    <RATE>0.4885</RATE>    <CHANGE>1.097</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Krone</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>NOK</CURRENCYCODE>    <COUNTRY>Norway</COUNTRY>    <RATE>0.3508</RATE>    <CHANGE>1.622</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Rand</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>ZAR</CURRENCYCODE>    <COUNTRY>South Africa</COUNTRY>    <RATE>0.2155</RATE>    <CHANGE>0.701</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Krona</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>SEK</CURRENCYCODE>    <COUNTRY>Sweden</COUNTRY>    <RATE>0.3413</RATE>    <CHANGE>1.276</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Franc</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>CHF</CURRENCYCODE>    <COUNTRY>Switzerland</COUNTRY>    <RATE>3.5964</RATE>    <CHANGE>1.416</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Dinar</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>JOD</CURRENCYCODE>    <COUNTRY>Jordan</COUNTRY>    <RATE>4.8468</RATE>    <CHANGE>1.163</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Pound</NAME>    <UNIT>10</UNIT>    <CURRENCYCODE>LBP</CURRENCYCODE>    <COUNTRY>Lebanon</COUNTRY>    <RATE>0.0227</RATE>    <CHANGE>0.889</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Pound</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>EGP</CURRENCYCODE>    <COUNTRY>Egypt</COUNTRY>    <RATE>0.1830</RATE>    <CHANGE>1.049</CHANGE>  </CURRENCY></CURRENCIES>".as_bytes();
        let context_bytes = forex
            .encode_context(&ForexContextArgs {
                timestamp: 1656374400,
            })
            .expect("should be able to encode");
        let context =
            Forex::decode_context(&context_bytes).expect("should be able to decode bytes");
        let bytes = forex
            .transform_http_response_body(body, &context.payload)
            .expect("should be able to transform the body");
        let result = Forex::decode_response(&bytes);

        assert!(matches!(result, Ok(map) if map["EUR"] == 10_579));
    }

    /// Test that response decoding works correctly.
    #[test]
    fn decode_transformed_http_response() {
        let hex_string = "4449444c026d016c0200710178010001034555520100000000000000";
        let bytes = hex::decode(hex_string).expect("should be able to decode");
        let result = Forex::decode_response(&bytes);
        assert!(matches!(result, Ok(map) if map["EUR"] == 1));
    }
}
