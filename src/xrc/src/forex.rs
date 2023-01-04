#[cfg(test)]
mod test;

use chrono::naive::NaiveDateTime;
use ic_cdk::export::candid::{
    decode_args, decode_one, encode_args, encode_one, CandidType, Deserialize, Error as CandidError,
};
use std::cmp::min;
use std::collections::{HashSet, VecDeque};
use std::mem::size_of_val;
use std::{collections::HashMap, convert::TryInto};

use crate::{
    candid::{Asset, AssetClass, ExchangeRateError},
    median, standard_deviation, utils, AllocatedBytes, ExtractError, QueriedExchangeRate, ONE_KIB,
    RATE_UNIT, USD,
};

/// The IMF SDR weights used to compute the XDR rate.
pub(crate) const USD_XDR_WEIGHT_PER_MILLION: u128 = 582_520;
pub(crate) const EUR_XDR_WEIGHT_PER_MILLION: u128 = 386_710;
pub(crate) const CNY_XDR_WEIGHT_PER_MILLION: u128 = 1_017_400;
pub(crate) const JPY_XDR_WEIGHT_PER_MILLION: u128 = 11_900_000;
pub(crate) const GBP_XDR_WEIGHT_PER_MILLION: u128 = 85_946;

/// The CMC uses a computed XDR (CXDR) rate based on the IMF SDR weights.
pub(crate) const COMPUTED_XDR_SYMBOL: &str = "CXDR";

/// Maximal number of days to keep around in the [ForexRatesCollector]
const MAX_COLLECTION_DAYS: usize = 2;

/// A map of multiple forex rates with one source per forex. The key is the forex symbol and the value is the corresponding rate.
pub type ForexRateMap = HashMap<String, u64>;

/// A map of multiple forex rates with possibly multiple sources per forex. The key is the forex symbol and the value is the corresponding rate and the number of sources used to compute it.
pub(crate) type ForexMultiRateMap = HashMap<String, QueriedExchangeRate>;

impl AllocatedBytes for ForexMultiRateMap {
    fn allocated_bytes(&self) -> usize {
        size_of_val(self)
            + self.iter().fold(0, |acc, (key, rate)| {
                acc + size_of_val(key) + key.len() + rate.allocated_bytes()
            })
    }
}

/// The forex rate storage struct. Stores a map of <timestamp, [ForexMultiRateMap]>.
#[derive(CandidType, Deserialize, Clone, Debug)]
pub(crate) struct ForexRateStore {
    rates: HashMap<u64, ForexMultiRateMap>,
}

/// A forex rate collector for a specific day. Allows the collection of multiple rates from different sources, and outputs the
/// aggregated [ForexMultiRateMap] to be stored.
#[derive(Clone, Debug)]
struct OneDayRatesCollector {
    rates: HashMap<String, Vec<u64>>,
    sources: HashSet<String>,
    timestamp: u64,
}

/// A forex rate collector. Allows the collection of rates for the last [MAX_COLLECTION_DAYS] days.
#[derive(Clone, Debug)]
pub struct ForexRatesCollector {
    days: VecDeque<OneDayRatesCollector>,
}

const SECONDS_PER_HOUR: u64 = 60 * 60;
const SECONDS_PER_DAY: u64 = SECONDS_PER_HOUR * 24;
const TIMEZONE_AOE_SHIFT_SECONDS: i64 = -12 * SECONDS_PER_DAY as i64;
const MAX_DAYS_TO_GO_BACK: u64 = 4;

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

            /// This method invokes the forex's [IsForex::supports_ipv6] function.
            pub fn supports_ipv6(&self) -> bool {
                match self {
                    $(Forex::$name(forex) => forex.supports_ipv6()),*,
                }
            }

            /// This method invokes the forex's [IsForex::offset_timestamp_to_timezone] function.
            pub fn offset_timestamp_to_timezone(&self, timestamp: u64) -> u64 {
                match self {
                    $(Forex::$name(forex) => forex.offset_timestamp_to_timezone(timestamp)),*,
                }
            }

            /// This method invokes the forex's [IsForex::offset_timestamp_for_query] function.
            pub fn offset_timestamp_for_query(&self, timestamp: u64) -> u64 {
                match self {
                    $(Forex::$name(forex) => forex.offset_timestamp_for_query(timestamp)),*,
                }
            }

            /// This method invokes the exchange's [IsExchange::max_response_bytes] function.
            pub fn max_response_bytes(&self) -> u64 {
                match self {
                    $(Forex::$name(forex) => forex.max_response_bytes()),*,
                }
            }

            /// This method returns whether the exchange should be called. Availability
            /// is determined by whether or not the `ipv4-support` flag was used to compile the
            /// canister or the exchange supports IPv6 out-of-the-box.
            ///
            /// NOTE: This will be removed when IPv4 support is added to HTTP outcalls.
            pub fn is_available(&self) -> bool {
                utils::is_ipv4_support_available() || self.supports_ipv6()
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

impl ForexRateStore {
    pub fn new() -> Self {
        Self {
            rates: HashMap::new(),
        }
    }

    /// Returns the exchange rate for the given two forex assets and a given timestamp, or None if a rate cannot be found.
    pub(crate) fn get(
        &self,
        requested_timestamp: u64,
        current_timestamp: u64,
        base_asset: &str,
        quote_asset: &str,
    ) -> Result<QueriedExchangeRate, GetForexRateError> {
        // Normalize timestamp to the beginning of the day.
        let mut timestamp = (requested_timestamp / SECONDS_PER_DAY) * SECONDS_PER_DAY;

        // If today's date is requested, and the day is not over anywhere on Earth, use yesterday's date
        // Get the normalized timestamp for yesterday.
        let yesterday = (current_timestamp as i64 + TIMEZONE_AOE_SHIFT_SECONDS) as u64
            / SECONDS_PER_DAY
            * SECONDS_PER_DAY;
        if timestamp > SECONDS_PER_DAY && yesterday == timestamp {
            timestamp -= SECONDS_PER_DAY;
        }

        let base_asset = base_asset.to_uppercase();
        let quote_asset = quote_asset.to_uppercase();
        if base_asset == quote_asset {
            return Ok(QueriedExchangeRate {
                base_asset: Asset {
                    symbol: base_asset.clone(),
                    class: AssetClass::FiatCurrency,
                },
                quote_asset: Asset {
                    symbol: base_asset,
                    class: AssetClass::FiatCurrency,
                },
                timestamp,
                rates: vec![RATE_UNIT],
                base_asset_num_queried_sources: 0,
                base_asset_num_received_rates: 0,
                quote_asset_num_queried_sources: 0,
                quote_asset_num_received_rates: 0,
                forex_timestamp: Some(timestamp),
            });
        }

        let mut go_back_days = 0;

        // If we can't find forex rates for the requested timestamp, we may go back up to [MAX_DAYS_TO_GO_BACK] days as it might have been a weekend or a holiday.
        while go_back_days <= MAX_DAYS_TO_GO_BACK {
            let query_timestamp = timestamp - SECONDS_PER_DAY * go_back_days;
            go_back_days += 1;
            if let Some(rates_for_timestamp) = self.rates.get(&query_timestamp) {
                let base = rates_for_timestamp.get(&base_asset);
                let quote = rates_for_timestamp.get(&quote_asset);

                match (base, quote) {
                    (Some(base_rate), Some(quote_rate)) => {
                        return Ok(base_rate.clone() / quote_rate.clone())
                    }
                    (Some(base_rate), None) => {
                        // If the quote asset is USD, it should not be present in the map and the base rate already uses USD as the quote asset.
                        if quote_asset == USD {
                            return Ok(base_rate.clone());
                        } else {
                            return Err(GetForexRateError::CouldNotFindQuoteAsset(
                                timestamp,
                                quote_asset.to_string(),
                            ));
                        }
                    }
                    (None, Some(_)) => {
                        return Err(GetForexRateError::CouldNotFindBaseAsset(
                            timestamp,
                            base_asset.to_string(),
                        ))
                    }
                    (None, None) => {
                        if quote_asset == USD {
                            return Err(GetForexRateError::CouldNotFindBaseAsset(
                                timestamp,
                                base_asset.to_string(),
                            ));
                        } else {
                            return Err(GetForexRateError::CouldNotFindAssets(
                                timestamp,
                                base_asset.to_string(),
                                quote_asset.to_string(),
                            ));
                        }
                    }
                }
            }
        }
        // If we got here, no rate is found for this timestamp within a range of [MAX_DAYS_TO_GO_BACK] days before it.
        Err(GetForexRateError::InvalidTimestamp(timestamp))
    }

    /// Puts or updates rates for a given timestamp. If rates already exist for the given timestamp, only rates for which a new rate with higher number of sources are replaced.
    pub(crate) fn put(&mut self, timestamp: u64, rates: ForexMultiRateMap) {
        // Normalize timestamp to the beginning of the day.
        let timestamp = (timestamp / SECONDS_PER_DAY) * SECONDS_PER_DAY;

        if let Some(ratesmap) = self.rates.get_mut(&timestamp) {
            // Update only the rates where the number of sources is higher.
            rates.into_iter().for_each(|(symbol, rate)| {
                // We should never insert rates for USD.
                if symbol != USD {
                    ratesmap
                        .entry(symbol)
                        .and_modify(|v| {
                            if v.base_asset_num_received_rates < rate.base_asset_num_received_rates
                            {
                                *v = rate.clone()
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

impl AllocatedBytes for ForexRateStore {
    fn allocated_bytes(&self) -> usize {
        size_of_val(&self.rates)
            + self.rates.iter().fold(0, |acc, (timestamp, multi_map)| {
                acc + size_of_val(timestamp) + multi_map.allocated_bytes()
            })
    }
}

impl OneDayRatesCollector {
    fn new(timestamp: u64) -> Self {
        Self {
            rates: HashMap::new(),
            sources: HashSet::new(),
            timestamp,
        }
    }

    /// Updates the collected rates with a new set of rates.
    pub(crate) fn update(&mut self, source: String, rates: ForexRateMap) {
        if !rates.is_empty() {
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
            self.sources.insert(source);
        }
    }

    /// Extracts all the up-to-date rates.
    pub(crate) fn get_rates_map(&self) -> ForexMultiRateMap {
        let mut rates: ForexMultiRateMap = self
            .rates
            .iter()
            .map(|(k, v)| {
                (
                    k.to_string(),
                    QueriedExchangeRate {
                        base_asset: Asset {
                            symbol: k.to_string(),
                            class: AssetClass::FiatCurrency,
                        },
                        quote_asset: Asset {
                            symbol: USD.to_string(),
                            class: AssetClass::FiatCurrency,
                        },
                        timestamp: self.timestamp,
                        rates: v.clone(),
                        base_asset_num_queried_sources: FOREX_SOURCES.len(),
                        base_asset_num_received_rates: v.len(),
                        quote_asset_num_queried_sources: FOREX_SOURCES.len(),
                        quote_asset_num_received_rates: v.len(),
                        forex_timestamp: Some(self.timestamp),
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
    fn get_computed_xdr_rate(&self) -> Option<QueriedExchangeRate> {
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
            let eur_rate = median(eur_rates) as u128;
            let cny_rate = median(cny_rates) as u128;
            let jpy_rate = median(jpy_rates) as u128;
            let gbp_rate = median(gbp_rates) as u128;

            // The factor `RATE_UNIT` is the scaled USD/USD rate, i.e., the rate 1.00 times `RATE_UNIT`.
            let xdr_rate = (USD_XDR_WEIGHT_PER_MILLION
                .saturating_mul(RATE_UNIT as u128)
                .saturating_add(EUR_XDR_WEIGHT_PER_MILLION.saturating_mul(eur_rate))
                .saturating_add(CNY_XDR_WEIGHT_PER_MILLION.saturating_mul(cny_rate))
                .saturating_add(JPY_XDR_WEIGHT_PER_MILLION.saturating_mul(jpy_rate))
                .saturating_add(GBP_XDR_WEIGHT_PER_MILLION.saturating_mul(gbp_rate)))
            .saturating_div(1_000_000u128) as u64;

            let xdr_num_sources = min(
                min(min(eur_rates.len(), cny_rates.len()), jpy_rates.len()),
                gbp_rates.len(),
            );

            let weighted_eur_std_dev =
                EUR_XDR_WEIGHT_PER_MILLION.saturating_mul(standard_deviation(eur_rates) as u128);
            let weighted_cny_std_dev =
                CNY_XDR_WEIGHT_PER_MILLION.saturating_mul(standard_deviation(cny_rates) as u128);
            let weighted_jpy_std_dev =
                JPY_XDR_WEIGHT_PER_MILLION.saturating_mul(standard_deviation(jpy_rates) as u128);
            let weighted_gbp_std_dev =
                GBP_XDR_WEIGHT_PER_MILLION.saturating_mul(standard_deviation(gbp_rates) as u128);

            // Assuming independence, the variance is the sum of squared weighted standard deviations
            // because Var(aX + bY) = a^2*Var(X) + b^2*Var(Y) for independent X and Y.
            let variance = (weighted_eur_std_dev
                .saturating_pow(2)
                .saturating_add(weighted_cny_std_dev.saturating_pow(2))
                .saturating_add(weighted_jpy_std_dev.saturating_pow(2))
                .saturating_add(weighted_gbp_std_dev.saturating_pow(2)))
            .saturating_div(1_000_000_000_000); // Removing the factor (10^6)^2 due to the weight scaling.

            let difference = (variance as f64).sqrt() as u64;

            Some(QueriedExchangeRate {
                base_asset: Asset {
                    symbol: COMPUTED_XDR_SYMBOL.to_string(),
                    class: AssetClass::FiatCurrency,
                },
                quote_asset: Asset {
                    symbol: USD.to_string(),
                    class: AssetClass::FiatCurrency,
                },
                timestamp: self.timestamp,
                rates: vec![
                    xdr_rate.saturating_sub(difference),
                    xdr_rate,
                    xdr_rate.saturating_add(difference),
                ],
                base_asset_num_queried_sources: FOREX_SOURCES.len(),
                base_asset_num_received_rates: xdr_num_sources,
                quote_asset_num_queried_sources: FOREX_SOURCES.len(),
                quote_asset_num_received_rates: xdr_num_sources,
                forex_timestamp: Some(self.timestamp),
            })
        } else {
            None
        }
    }
}

impl ForexRatesCollector {
    pub fn new() -> ForexRatesCollector {
        ForexRatesCollector {
            days: VecDeque::with_capacity(MAX_COLLECTION_DAYS),
        }
    }

    /// Updates the collected rates with a new set of rates. The provided timestamp must exist in the collector or be newer than the existing ones. The function returns true if the collector has been updated, or false if the timestamp is too old.
    pub(crate) fn update(&mut self, source: String, timestamp: u64, rates: ForexRateMap) -> bool {
        let timestamp = (timestamp / SECONDS_PER_DAY) * SECONDS_PER_DAY;

        let mut create_new = false;
        if let Some(one_day_collector) = self.days.iter_mut().find(|odc| odc.timestamp == timestamp)
        {
            // Already has a collector for this day
            one_day_collector.update(source, rates);
            return true;
        } else if let Some(max_time) = self.days.iter().map(|odc| odc.timestamp).max() {
            if timestamp > max_time {
                // New day
                create_new = true;
            } // Else, timestamp is too old
        } else {
            // Collector is empty
            create_new = true;
        }
        if create_new {
            // Create a new entry for a new day
            // Remove oldest day if there are [MAX_COLLECTION_DAYS] entries
            let mut new_collector = OneDayRatesCollector::new(timestamp);
            new_collector.update(source, rates);
            if self.days.len() == MAX_COLLECTION_DAYS {
                self.days.pop_front();
            }
            self.days.push_back(new_collector);
            true
        } else {
            false
        }
    }

    /// Extracts all existing rates for the given timestamp, if it exists in this collector.
    pub(crate) fn get_rates_map(&self, timestamp: u64) -> Option<ForexMultiRateMap> {
        self.days
            .iter()
            .find(|one_day_collector| one_day_collector.timestamp == timestamp)
            .map(|one_day_collector| one_day_collector.get_rates_map())
    }

    /// Return the list of sources used for a given timestamp.
    pub(crate) fn get_sources(&self, timestamp: u64) -> Option<Vec<String>> {
        self.days
            .iter()
            .find(|one_day_collector| one_day_collector.timestamp == timestamp)
            .map(|one_day_collector| one_day_collector.sources.clone().into_iter().collect())
    }
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
                .map(|(symbol, value)| {
                    (
                        symbol.to_string(),
                        // Use u128 to avoid potential overflow
                        ((RATE_UNIT as u128 * *value as u128) / *usd_value as u128) as u64,
                    )
                })
                .collect()),
            None => Err(ExtractError::RateNotFound {
                filter: "No USD rate".to_string(),
            }),
        }
    }

    /// Indicates if the forex source supports IPv6.
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

    /// Returns the reference timezone offset for the forex source.
    fn get_utc_offset(&self) -> i16;

    /// Returns the timestamp in the timezone of the source, given the UTC time `current_timestamp`.
    fn offset_timestamp_to_timezone(&self, current_timestamp: u64) -> u64 {
        (current_timestamp as i64 + (self.get_utc_offset() as i64 * SECONDS_PER_HOUR as i64)) as u64
    }

    /// Returns the actual timestamp that needs to be used in order to query the given timestamp's rates.
    /// (Some sources expect a different date, usually for the day after)
    fn offset_timestamp_for_query(&self, timestamp: u64) -> u64 {
        timestamp
    }

    fn max_response_bytes(&self) -> u64 {
        ONE_KIB
    }
}

/// Monetary Authority Of Singapore
#[derive(Deserialize)]
struct MonetaryAuthorityOfSingaporeResponse {
    result: MonetaryAuthorityOfSingaporeResponseResult,
}

#[derive(Deserialize)]
struct MonetaryAuthorityOfSingaporeResponseResult {
    records: Vec<HashMap<String, String>>,
}

impl IsForex for MonetaryAuthorityOfSingapore {
    fn format_timestamp(&self, timestamp: u64) -> String {
        format!(
            "{}",
            NaiveDateTime::from_timestamp(timestamp.try_into().unwrap_or(0), 0).format("%Y-%m-%d")
        )
    }

    fn extract_rate(&self, bytes: &[u8], timestamp: u64) -> Result<ForexRateMap, ExtractError> {
        let response = serde_json::from_slice::<MonetaryAuthorityOfSingaporeResponse>(bytes)
            .map_err(|err| ExtractError::json_deserialize(bytes, err.to_string()))?;
        let timestamp = (timestamp / SECONDS_PER_DAY) * SECONDS_PER_DAY;

        let map = response.result.records.get(0).ok_or_else(|| {
            ExtractError::json_deserialize(bytes, "Missing record index".to_string())
        })?;

        let extracted_timestamp = {
            let maybe_end_of_day = map.get("end_of_day");
            let end_of_day = match maybe_end_of_day {
                Some(end_of_day) => NaiveDateTime::parse_from_str(
                    &(end_of_day.to_string() + " 00:00:00"),
                    "%Y-%m-%d %H:%M:%S",
                )
                .unwrap_or_else(|_| NaiveDateTime::from_timestamp(0, 0)),
                None => NaiveDateTime::from_timestamp(0, 0),
            };
            end_of_day.timestamp() as u64
        };

        if extracted_timestamp != timestamp {
            return Err(ExtractError::RateNotFound {
                filter: "Invalid timestamp".to_string(),
            });
        }

        let mut values = map
            .iter()
            .filter_map(|(key, value)| {
                if !key.contains("_sgd") {
                    return None;
                }

                match value.parse::<f64>() {
                    Ok(rate) => match key.split('_').next() {
                        Some(symbol) => {
                            let scaled_rate = if key.ends_with("_100") {
                                (rate * (RATE_UNIT as f64 / 100.0)) as u64
                            } else {
                                (rate * RATE_UNIT as f64) as u64
                            };

                            Some((symbol.to_uppercase(), scaled_rate))
                        }
                        None => None,
                    },
                    Err(_) => None,
                }
            })
            .collect::<ForexRateMap>();
        values.insert("SGD".to_string(), RATE_UNIT);
        self.normalize_to_usd(&values)
    }

    fn get_base_url(&self) -> &str {
        "https://eservices.mas.gov.sg/api/action/datastore/search.json?resource_id=95932927-c8bc-4e7a-b484-68a66a24edfe&limit=100&filters[end_of_day]=DATE"
    }

    fn supports_ipv6(&self) -> bool {
        true
    }

    fn get_utc_offset(&self) -> i16 {
        8
    }

    fn max_response_bytes(&self) -> u64 {
        // 3 KiB
        ONE_KIB * 3
    }
}

/// Central Bank of Myanmar
#[derive(Debug, Deserialize)]
struct CentralBankOfMyanmarResponse {
    timestamp: u64,
    rates: HashMap<String, String>,
}

impl IsForex for CentralBankOfMyanmar {
    fn format_timestamp(&self, timestamp: u64) -> String {
        format!(
            "{}",
            NaiveDateTime::from_timestamp(timestamp.try_into().unwrap_or(0), 0).format("%d-%m-%Y")
        )
    }

    fn extract_rate(&self, bytes: &[u8], timestamp: u64) -> Result<ForexRateMap, ExtractError> {
        let response = serde_json::from_slice::<CentralBankOfMyanmarResponse>(bytes)
            .map_err(|err| ExtractError::json_deserialize(bytes, err.to_string()))?;
        let timestamp = (timestamp / SECONDS_PER_DAY) * SECONDS_PER_DAY;

        if response.timestamp != timestamp {
            return Err(ExtractError::RateNotFound {
                filter: "Invalid timestamp".to_string(),
            });
        }

        let values = response
            .rates
            .iter()
            .filter_map(|(asset, rate)| {
                let parsed = rate.replace(',', "").parse::<f64>().ok()?;
                let rate = (parsed * RATE_UNIT as f64) as u64;
                Some((asset.to_uppercase(), rate))
            })
            .collect::<ForexRateMap>();
        self.normalize_to_usd(&values)
    }

    fn get_base_url(&self) -> &str {
        "https://forex.cbm.gov.mm/api/history/DATE"
    }

    fn supports_ipv6(&self) -> bool {
        true
    }

    fn get_utc_offset(&self) -> i16 {
        // Myanmar timezone is UTC+6.5. To avoid using floating point types here, we use a truncated offset.
        6
    }

    fn max_response_bytes(&self) -> u64 {
        // 3KiB - this is need to get past the http body size limit
        ONE_KIB * 3
    }
}

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
        let timestamp = (timestamp / SECONDS_PER_DAY) * SECONDS_PER_DAY;

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
        ((timestamp / SECONDS_PER_DAY) + 1) * SECONDS_PER_DAY
    }

    /// Responses are between 20-25 KiB. Set to 30 to give some leeway.
    fn max_response_bytes(&self) -> u64 {
        // 30 KiB
        ONE_KIB * 30
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
                        (item.rate * RATE_UNIT as f64) as u64 / item.unit,
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

    fn get_utc_offset(&self) -> i16 {
        2
    }

    fn max_response_bytes(&self) -> u64 {
        ONE_KIB * 10
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

/// Bank of Canada
#[derive(Debug, Deserialize)]
struct BankOfCanadaResponseSeriesDetail {
    label: String,
}

#[derive(Debug, Deserialize)]
struct BankOfCanadaResponseObservation {
    d: String,
    #[serde(flatten)]
    rates: HashMap<String, BankOfCanadaResponseObservationValue>,
}

#[derive(Debug, Deserialize)]
struct BankOfCanadaResponseObservationValue {
    v: String,
}

#[derive(Debug, Deserialize)]
struct BankOfCanadaResponse {
    #[serde(rename(deserialize = "seriesDetail"))]
    series_detail: HashMap<String, BankOfCanadaResponseSeriesDetail>,
    observations: Vec<BankOfCanadaResponseObservation>,
}

impl IsForex for BankOfCanada {
    fn format_timestamp(&self, timestamp: u64) -> String {
        format!(
            "{}",
            NaiveDateTime::from_timestamp(timestamp.try_into().unwrap_or(0), 0).format("%Y-%m-%d")
        )
    }

    fn extract_rate(&self, bytes: &[u8], timestamp: u64) -> Result<ForexRateMap, ExtractError> {
        let response = serde_json::from_slice::<BankOfCanadaResponse>(bytes)
            .map_err(|err| ExtractError::json_deserialize(bytes, err.to_string()))?;

        let timestamp = (timestamp / SECONDS_PER_DAY) * SECONDS_PER_DAY;
        let mut extracted_timestamp: u64;
        let mut values = ForexRateMap::new();
        for observation in response.observations.iter() {
            extracted_timestamp = NaiveDateTime::parse_from_str(
                &(observation.d.to_string() + " 00:00:00"),
                "%Y-%m-%d %H:%M:%S",
            )
            .unwrap_or_else(|_| NaiveDateTime::from_timestamp(0, 0))
            .timestamp() as u64;
            if extracted_timestamp != timestamp {
                return Err(ExtractError::RateNotFound {
                    filter: "Invalid timestamp".to_string(),
                });
            }

            observation.rates.iter().for_each(|(key, value)| {
                let detail = match response.series_detail.get(key) {
                    Some(detail) => detail,
                    None => return,
                };

                let symbol = match detail.label.split('/').next() {
                    Some(symbol) => symbol,
                    None => return,
                };

                let value = match value.v.parse::<f64>() {
                    Ok(value) => value,
                    Err(_) => return,
                };

                values.insert(symbol.to_uppercase(), (value * RATE_UNIT as f64) as u64);
            });
        }

        values.insert("CAD".to_string(), RATE_UNIT);
        self.normalize_to_usd(&values)
    }

    fn get_base_url(&self) -> &str {
        "https://www.bankofcanada.ca/valet/observations/group/FX_RATES_DAILY/json?start_date=DATE&end_date=DATE"
    }

    fn get_utc_offset(&self) -> i16 {
        // Using the westmost timezone (PST)
        -8
    }

    fn max_response_bytes(&self) -> u64 {
        ONE_KIB * 10
    }
}

/// Central Bank of Uzbekistan
#[derive(Debug, Deserialize)]
struct CentralBankOfUzbekistanDetail {
    #[serde(rename(deserialize = "Ccy"))]
    currency: String,
    #[serde(rename(deserialize = "Rate"))]
    rate: String,
    #[serde(rename(deserialize = "Date"))]
    date: String,
}

impl IsForex for CentralBankOfUzbekistan {
    fn format_timestamp(&self, timestamp: u64) -> String {
        format!(
            "{}",
            NaiveDateTime::from_timestamp(timestamp.try_into().unwrap_or(0), 0).format("%Y-%m-%d")
        )
    }

    fn extract_rate(&self, bytes: &[u8], timestamp: u64) -> Result<ForexRateMap, ExtractError> {
        let response = serde_json::from_slice::<Vec<CentralBankOfUzbekistanDetail>>(bytes)
            .map_err(|err| ExtractError::json_deserialize(bytes, err.to_string()))?;

        let timestamp = (timestamp / SECONDS_PER_DAY) * SECONDS_PER_DAY;
        let mut values = ForexRateMap::new();

        for detail in response {
            let extracted_timestamp =
                NaiveDateTime::parse_from_str(&(detail.date + " 00:00:00"), "%d.%m.%Y %H:%M:%S")
                    .unwrap_or_else(|_| NaiveDateTime::from_timestamp(0, 0))
                    .timestamp() as u64;
            if extracted_timestamp != timestamp {
                return Err(ExtractError::RateNotFound {
                    filter: "Invalid timestamp".to_string(),
                });
            }

            let rate = match detail.rate.parse::<f64>() {
                Ok(rate) => (rate * RATE_UNIT as f64) as u64,
                Err(_) => continue,
            };

            values.insert(detail.currency, rate);
        }

        self.normalize_to_usd(&values)
    }

    fn get_base_url(&self) -> &str {
        "https://cbu.uz/ru/arkhiv-kursov-valyut/json/all/DATE/"
    }

    fn get_utc_offset(&self) -> i16 {
        5
    }

    fn max_response_bytes(&self) -> u64 {
        ONE_KIB * 30
    }
}
