mod australia;

use chrono::naive::NaiveDateTime;
use ic_cdk::export::candid::{
    decode_args, decode_one, encode_args, encode_one, CandidType, Deserialize, Error as CandidError,
};
use ic_xrc_types::{Asset, AssetClass, ExchangeRateError};
use std::cmp::min;
use std::collections::{HashSet, VecDeque};
use std::mem::size_of_val;
use std::{collections::HashMap, convert::TryInto};

use crate::{
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
const TIMEZONE_AOE_SHIFT_HOURS: i16 = 12;
const MAX_DAYS_TO_GO_BACK: u64 = 4;

/// This macro generates the necessary boilerplate when adding a forex data source to this module.
macro_rules! forex {
    ($($name:ident),*) => {
        /// Enum that contains all of the possible forex sources.
        #[derive(Debug, PartialEq)]
        pub enum Forex {
            $(
                #[allow(missing_docs)]
                #[allow(dead_code)]
                $name($name),
            )*
        }

        $(
            #[derive(Debug, PartialEq)]
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

            /// This method invokes the forex's [IsForex::get_utc_offset] function.
            pub fn get_utc_offset(&self) -> i16 {
                match self {
                    $(Forex::$name(forex) => forex.get_utc_offset()),*,
                }
            }

            /// This method invokes the forex's [IsForex::offset_timestamp_to_timezone] function.
            pub fn offset_timestamp_to_timezone(&self, timestamp: u64) -> u64 {
                if cfg!(feature = "disable-forex-timezone-offset") {
                    return timestamp;
                }
                match self {
                    $(Forex::$name(forex) => forex.offset_timestamp_to_timezone(timestamp)),*,
                }
            }

            /// This method invokes the forex's [IsForex::offset_timestamp_for_query] function.
            pub fn offset_timestamp_for_query(&self, timestamp: u64) -> u64 {
                if cfg!(feature = "disable-forex-timezone-offset") {
                    return timestamp;
                }
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

forex! { MonetaryAuthorityOfSingapore, CentralBankOfMyanmar, CentralBankOfBosniaHerzegovina, EuropeanCentralBank, BankOfCanada, CentralBankOfUzbekistan, ReserveBankOfAustralia }

#[derive(Debug)]
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

    fn shift_to_latest_source_eod(requested_timestamp: u64, current_timestamp: u64) -> u64 {
        // We avoid fetching rates for today if today is not over for any of the sources we use.
        // Therefore, if the current time means the day is not over for the source at the western-most timezone,
        // we use the normalized timestamp for yesterday.
        let max_shift_hours = FOREX_SOURCES
            .iter()
            .map(|src| src.get_utc_offset())
            .min()
            .unwrap_or(-(TIMEZONE_AOE_SHIFT_HOURS as i16)) as i64;

        let shift_to_latest_source_eod =
            (SECONDS_PER_DAY as i64 + (max_shift_hours * SECONDS_PER_HOUR as i64)) as u64;
        let requested_day_end_on_all_sources =
            requested_timestamp.saturating_add(shift_to_latest_source_eod);
        if current_timestamp < requested_day_end_on_all_sources {
            requested_timestamp.saturating_sub(SECONDS_PER_DAY)
        } else {
            requested_timestamp
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
        let mut requested_timestamp = (requested_timestamp / SECONDS_PER_DAY) * SECONDS_PER_DAY;

        if !cfg!(feature = "disable-forex-timezone-offset") {
            requested_timestamp =
                Self::shift_to_latest_source_eod(requested_timestamp, current_timestamp);
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
                timestamp: requested_timestamp,
                rates: vec![RATE_UNIT],
                base_asset_num_queried_sources: 0,
                base_asset_num_received_rates: 0,
                quote_asset_num_queried_sources: 0,
                quote_asset_num_received_rates: 0,
                forex_timestamp: Some(requested_timestamp),
            });
        }

        let mut go_back_days = 0;

        // If we can't find forex rates for the requested timestamp, we may go back up to [MAX_DAYS_TO_GO_BACK] days as it might have been a weekend or a holiday.
        while go_back_days <= MAX_DAYS_TO_GO_BACK {
            let query_timestamp =
                requested_timestamp.saturating_sub(SECONDS_PER_DAY * go_back_days);
            go_back_days += 1;
            if let Some(rates_for_timestamp) = self.rates.get(&query_timestamp) {
                if quote_asset == USD {
                    let base = rates_for_timestamp.get(&base_asset);
                    return base.cloned().ok_or_else(|| {
                        GetForexRateError::CouldNotFindBaseAsset(
                            requested_timestamp,
                            base_asset.to_string(),
                        )
                    });
                }

                if base_asset == USD {
                    let quote = rates_for_timestamp.get(&quote_asset);
                    return quote.map(|rate| rate.inverted()).ok_or_else(|| {
                        GetForexRateError::CouldNotFindQuoteAsset(
                            requested_timestamp,
                            quote_asset.to_string(),
                        )
                    });
                }

                let base = rates_for_timestamp.get(&base_asset);
                let quote = rates_for_timestamp.get(&quote_asset);

                match (base, quote) {
                    (Some(base_rate), Some(quote_rate)) => {
                        return Ok(base_rate.clone() / quote_rate.clone());
                    }
                    (Some(_), None) => {
                        return Err(GetForexRateError::CouldNotFindQuoteAsset(
                            requested_timestamp,
                            quote_asset.to_string(),
                        ));
                    }
                    (None, Some(_)) => {
                        return Err(GetForexRateError::CouldNotFindBaseAsset(
                            requested_timestamp,
                            base_asset.to_string(),
                        ));
                    }
                    (None, None) => {
                        return Err(GetForexRateError::CouldNotFindAssets(
                            requested_timestamp,
                            base_asset.to_string(),
                            quote_asset.to_string(),
                        ));
                    }
                }
            }
        }
        // If we got here, no rate is found for this timestamp within a range of [MAX_DAYS_TO_GO_BACK] days before it.
        Err(GetForexRateError::InvalidTimestamp(requested_timestamp))
    }

    /// Inserts or updates rates for a given timestamp. If rates already exist for the given timestamp,
    /// only rates for which a new rate with a higher number of sources are replaced.
    pub(crate) fn put(&mut self, timestamp: u64, rates: ForexMultiRateMap) {
        // Normalize timestamp to the beginning of the day.
        let timestamp = (timestamp / SECONDS_PER_DAY) * SECONDS_PER_DAY;

        if let Some(ratesmap) = self.rates.get_mut(&timestamp) {
            // Update only the rates where the number of sources is higher.
            rates.into_iter().for_each(|(symbol, rate)| {
                ratesmap
                    .entry(symbol)
                    .and_modify(|v| {
                        if v.base_asset_num_received_rates < rate.base_asset_num_received_rates {
                            *v = rate.clone()
                        }
                    })
                    .or_insert(rate);
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
        let num_queried_sources = FOREX_SOURCES.iter().filter(|e| e.is_available()).count();
        let mut rates: ForexMultiRateMap = self
            .rates
            .iter()
            .filter_map(|(k, v)| {
                if k == USD {
                    return None;
                }

                Some((
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
                        base_asset_num_queried_sources: num_queried_sources,
                        base_asset_num_received_rates: v.len(),
                        quote_asset_num_queried_sources: num_queried_sources,
                        quote_asset_num_received_rates: v.len(),
                        forex_timestamp: Some(self.timestamp),
                    },
                ))
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

            // The rates are set to [xdr_rate, xdr_rate, xdr_rate + difference], where
            // difference = sqrt(3*variance). This set has the required properties:
            // * The median of the set is xdr_rate.
            // * The variance of the set corresponds is 'variance'.
            let difference = ((3 * variance) as f64).sqrt() as u64;

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
                rates: vec![xdr_rate, xdr_rate, xdr_rate.saturating_add(difference)],
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

    /// Returns the timestamps that are currently sitting in the forex rates collector.
    pub(crate) fn get_timestamps(&self) -> Vec<u64> {
        self.days.iter().map(|day| day.timestamp).collect()
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
                let mut rate = (parsed * RATE_UNIT as f64) as u64;
                if asset == "JPY" {
                    rate /= 100;
                }
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

#[cfg(test)]
mod test {
    use ic_xrc_types::{ExchangeRate, ExchangeRateMetadata};
    use maplit::hashmap;

    use crate::utils::test::load_file;
    use crate::DECIMALS;

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
        let query_response = load_file("test-data/forex/monetary-authority-of-singapore.json");
        let timestamp: u64 = 1656374400;
        let extracted_rates = singapore.extract_rate(&query_response, timestamp);

        assert!(matches!(extracted_rates, Ok(ref rates) if rates["EUR"] == 1_058_173_944));
        assert!(matches!(extracted_rates, Ok(ref rates) if rates["JPY"] == 7_390_111));
    }

    /// The function tests if the [CentralBankOfMyanmar] struct returns the correct forex rate.
    #[test]
    fn extract_rate_from_myanmar() {
        let myanmar = CentralBankOfMyanmar;
        let query_response = load_file("test-data/forex/central-bank-of-myanmar.json");
        let timestamp: u64 = 1656374400;
        let extracted_rates = myanmar.extract_rate(&query_response, timestamp);
        assert!(matches!(extracted_rates, Ok(ref rates) if rates["EUR"] == 1_059_297_297));
        assert!(matches!(extracted_rates, Ok(ref rates) if rates["JPY"] == 7_369_729));
    }

    /// The function tests if the [CentralBankOfBosniaHerzegovina] struct returns the correct forex rate.
    #[test]
    fn extract_rate_from_bosnia() {
        let bosnia = CentralBankOfBosniaHerzegovina;
        let query_response = load_file("test-data/forex/central-bank-of-bosnia-herzegovina.json");
        let timestamp: u64 = 1656374400;
        let extracted_rates = bosnia.extract_rate(&query_response, timestamp);
        assert!(matches!(extracted_rates, Ok(ref rates) if rates["EUR"] == 1_057_200_262));
        assert!(matches!(extracted_rates, Ok(ref rates) if rates["JPY"] == 7_380_104));
    }

    /// The function tests if the [EuropeanCentralBank] struct returns the correct forex rate.
    #[test]
    fn extract_rate_from_ecb() {
        let ecb = EuropeanCentralBank;
        let query_response = load_file("test-data/forex/central-bank-of-europe.xml");
        let timestamp: u64 = 1664755200;
        let extracted_rates = ecb.extract_rate(&query_response, timestamp);

        assert!(matches!(extracted_rates, Ok(rates) if rates["EUR"] == 976_400_000));
    }

    /// The function tests if the [BankOfCanada] struct returns the correct forex rate.
    #[test]
    fn extract_rate_from_canada() {
        let canada = BankOfCanada;
        let query_response = load_file("test-data/forex/bank-of-canada.json");
        let timestamp: u64 = 1656374400;
        let extracted_rates = canada.extract_rate(&query_response, timestamp);

        assert!(matches!(extracted_rates, Ok(rates) if rates["EUR"] == 1_052_938_432));
    }

    /// The function tests if the [CentralBankOfUzbekistan] struct returns the correct forex rate.
    #[test]
    fn extract_rate_from_uzbekistan() {
        let uzbekistan = CentralBankOfUzbekistan;
        let query_response = load_file("test-data/forex/central-bank-of-uzbekistan.json");
        let timestamp: u64 = 1656374400;
        let extracted_rates = uzbekistan.extract_rate(&query_response, timestamp);

        assert!(matches!(extracted_rates, Ok(rates) if rates["EUR"] == 1_056_900_158));
    }

    /// Tests that the [OneDayRatesCollector] struct correctly collects rates and returns them.
    #[test]
    fn one_day_rate_collector_update_and_get() {
        // Create a collector, update three times, check median rates.
        let mut collector = OneDayRatesCollector {
            rates: HashMap::new(),
            timestamp: 1234,
            sources: HashSet::new(),
        };

        // Insert real values with the correct timestamp.
        let rates = hashmap! {
            "EUR".to_string() => 1_000_000_000,
            "SGD".to_string() => 100_000_000,
            "CHF".to_string() => 700_000_000,
        };
        collector.update("src1".to_string(), rates);
        let rates = hashmap! {
            "EUR".to_string() => 1_100_000_000,
            "SGD".to_string() => 1_000_000_000,
            "CHF".to_string() => 1_000_000_000,
        };
        collector.update("src2".to_string(), rates);
        let rates = hashmap! {
            "EUR".to_string() => 800_000_000,
            "SGD".to_string() => 1_300_000_000,
            "CHF".to_string() => 2_100_000_000,
        };
        collector.update("src3".to_string(), rates);

        let result = collector.get_rates_map();
        assert_eq!(result.len(), 3);
        result.values().for_each(|v| {
            let rate: ExchangeRate = v
                .clone()
                .try_into()
                .expect("The conversion to the ExchangeRate struct should work.");
            assert_eq!(rate.rate, RATE_UNIT);
            assert_eq!(rate.metadata.base_asset_num_received_rates, 3);
        });
    }

    /// Tests that the [ForexRatesCollector] struct correctly collects rates and returns them.
    #[test]
    fn rate_collector_update_and_get() {
        let mut collector = ForexRatesCollector::new();

        // Start by executing the same logic as for the [OneDayRatesCollector] to verify that the calls are relayed correctly
        let first_day_timestamp = (123456789 / SECONDS_PER_DAY) * SECONDS_PER_DAY;
        let rates = hashmap! {
            "EUR".to_string() => 1_000_000_000,
            "SGD".to_string() => 100_000_000,
            "CHF".to_string() => 700_000_000,
        };
        collector.update("src1".to_string(), first_day_timestamp, rates);
        let rates = hashmap! {
            "EUR".to_string() => 1_100_000_000,
            "SGD".to_string() => 1_000_000_000,
            "CHF".to_string() => 1_000_000_000,
        };
        collector.update("src2".to_string(), first_day_timestamp, rates);
        let rates = hashmap! {
            "EUR".to_string() => 800_000_000,
            "SGD".to_string() => 1_300_000_000,
            "CHF".to_string() => 2_100_000_000,
        };
        collector.update("src3".to_string(), first_day_timestamp, rates);

        let result = collector.get_rates_map(first_day_timestamp).unwrap();
        assert_eq!(result.len(), 3);
        result.values().for_each(|v| {
            let rate: ExchangeRate = v
                .clone()
                .try_into()
                .expect("The conversion to the ExchangeRate struct should work.");
            assert_eq!(rate.rate, RATE_UNIT);
            assert_eq!(rate.metadata.base_asset_num_received_rates, 3);
        });

        // Add a new day
        let second_day_timestamp = first_day_timestamp + SECONDS_PER_DAY;
        let test_rate: u64 = 700_000_000;
        let rates = hashmap! {
            "EUR".to_string() => test_rate,
            "SGD".to_string() => test_rate,
            "CHF".to_string() => test_rate,
        };
        collector.update("src1".to_string(), second_day_timestamp, rates);
        let result = collector.get_rates_map(second_day_timestamp).unwrap();
        assert_eq!(result.len(), 3);
        result.values().for_each(|v| {
            let rate: ExchangeRate = v
                .clone()
                .try_into()
                .expect("The conversion to the ExchangeRate struct should work.");
            assert_eq!(rate.rate, test_rate);
            assert_eq!(rate.metadata.base_asset_num_received_rates, 1);
        });

        // Add a third day and expect the first one to not be available
        let third_day_timestamp = second_day_timestamp + SECONDS_PER_DAY;
        let test_rate: u64 = 800_000_000;
        let rates = hashmap! {
            "EUR".to_string() => test_rate,
            "SGD".to_string() => test_rate,
            "CHF".to_string() => test_rate,
        };
        collector.update("src1".to_string(), third_day_timestamp, rates.clone());
        let result = collector.get_rates_map(third_day_timestamp).unwrap();
        assert_eq!(result.len(), 3);
        result.values().for_each(|v| {
            let rate: ExchangeRate = v
                .clone()
                .try_into()
                .expect("The conversion to the ExchangeRate struct should work.");
            assert_eq!(rate.rate, test_rate);
            assert_eq!(rate.metadata.base_asset_num_received_rates, 1);
        });
        assert!(collector.get_rates_map(first_day_timestamp).is_none());
        assert!(collector.get_rates_map(second_day_timestamp).is_some());

        // Try to add an old day and expect it to fail
        assert!(!collector.update("src1".to_string(), first_day_timestamp, rates));
    }

    #[test]
    fn ensure_rate_store_removes_usd_rates() {
        let mut collector = ForexRatesCollector::new();

        // Start by executing the same logic as for the [OneDayRatesCollector] to verify that the calls are relayed correctly.
        let timestamp = (123456789 / SECONDS_PER_DAY) * SECONDS_PER_DAY;
        collector.update(
            "src1".to_string(),
            timestamp,
            hashmap! {
                "EUR".to_string() => 1_000_000_000,
                "SGD".to_string() => 100_000_000,
                "CHF".to_string() => 700_000_000,
                USD.to_string() => RATE_UNIT
            },
        );
        collector.update(
            "src1".to_string(),
            timestamp,
            hashmap! {
                "EUR".to_string() => 1_100_000_000,
                "SGD".to_string() => 1_000_000_000,
                "CHF".to_string() => 1_000_000_000,
                USD.to_string() => RATE_UNIT
            },
        );
        collector.update(
            "src3".to_string(),
            timestamp,
            hashmap! {
                "EUR".to_string() => 800_000_000,
                "SGD".to_string() => 1_300_000_000,
                "CHF".to_string() => 2_100_000_000,
                USD.to_string() => RATE_UNIT
            },
        );

        let rates_map = collector
            .get_rates_map(timestamp)
            .expect("should be able to create a rates map");
        let mut store = ForexRateStore::new();
        store.put(timestamp, rates_map);

        let maybe_rate = store
            .rates
            .get(&timestamp)
            .expect("There should be an entry for the timestamp")
            .get(USD);
        assert!(maybe_rate.is_none());
    }

    /// Tests that the [ForexRatesStore] struct correctly updates rates for the same timestamp.
    #[test]
    fn rate_store_update() {
        // Create a store, update, check that only rates with more sources were updated.
        let mut store = ForexRateStore::new();
        store.put(
            1234,
            hashmap! {
                "EUR".to_string() =>
                    QueriedExchangeRate {
                        base_asset: Asset {
                            symbol: "EUR".to_string(),
                            class: AssetClass::FiatCurrency,
                        },
                        quote_asset: Asset {
                            symbol: USD.to_string(),
                            class: AssetClass::FiatCurrency,
                        },
                        timestamp: 1234,
                        rates: vec![800_000_000],
                        base_asset_num_queried_sources: 4,
                        base_asset_num_received_rates: 4,
                        quote_asset_num_queried_sources: 4,
                        quote_asset_num_received_rates: 4,
                        forex_timestamp: Some(1234),
                    },
                "SGD".to_string() =>
                    QueriedExchangeRate {
                        base_asset: Asset {
                            symbol: "SGD".to_string(),
                            class: AssetClass::FiatCurrency,
                        },
                        quote_asset: Asset {
                            symbol: USD.to_string(),
                            class: AssetClass::FiatCurrency,
                        },
                        timestamp: 1234,
                        rates: vec![1_000_000_000],
                        base_asset_num_queried_sources: 5,
                        base_asset_num_received_rates: 5,
                        quote_asset_num_queried_sources: 5,
                        quote_asset_num_received_rates: 5,
                        forex_timestamp: Some(1234),
                    },
                "CHF".to_string() =>
                    QueriedExchangeRate {
                        base_asset: Asset {
                            symbol: "CHF".to_string(),
                            class: AssetClass::FiatCurrency,
                        },
                        quote_asset: Asset {
                            symbol: USD.to_string(),
                            class: AssetClass::FiatCurrency,
                        },
                        timestamp: 1234,
                        rates: vec![2_100_000_000],
                        base_asset_num_queried_sources: 2,
                        base_asset_num_received_rates: 2,
                        quote_asset_num_queried_sources: 2,
                        quote_asset_num_received_rates: 2,
                        forex_timestamp: Some(1234),
                    },
                "CAD".to_string() =>
                    QueriedExchangeRate {
                        base_asset: Asset {
                            symbol: "CAD".to_string(),
                            class: AssetClass::FiatCurrency,
                        },
                        quote_asset: Asset {
                            symbol: USD.to_string(),
                            class: AssetClass::FiatCurrency,
                        },
                        timestamp: 1234,
                        rates: vec![2_500_000_000],
                        base_asset_num_queried_sources: 2,
                        base_asset_num_received_rates: 2,
                        quote_asset_num_queried_sources: 2,
                        quote_asset_num_received_rates: 2,
                        forex_timestamp: Some(1234),
                    },
            },
        );
        store.put(
            1234,
            hashmap! {
                "EUR".to_string() =>
                    QueriedExchangeRate {
                        base_asset: Asset {
                            symbol: "EUR".to_string(),
                            class: AssetClass::FiatCurrency,
                        },
                        quote_asset: Asset {
                            symbol: USD.to_string(),
                            class: AssetClass::FiatCurrency,
                        },
                        timestamp: 1234,
                        rates: vec![1_000_000_000],
                        base_asset_num_queried_sources: 5,
                        base_asset_num_received_rates: 5,
                        quote_asset_num_queried_sources: 5,
                        quote_asset_num_received_rates: 5,
                        forex_timestamp: Some(1234),
                    },
                "GBP".to_string() =>
                    QueriedExchangeRate {
                        base_asset: Asset {
                            symbol: "GBP".to_string(),
                            class: AssetClass::FiatCurrency,
                        },
                        quote_asset: Asset {
                            symbol: USD.to_string(),
                            class: AssetClass::FiatCurrency,
                        },
                        timestamp: 1234,
                        rates: vec![1_000_000_000],
                        base_asset_num_queried_sources: 2,
                        base_asset_num_received_rates: 2,
                        quote_asset_num_queried_sources: 2,
                        quote_asset_num_received_rates: 2,
                        forex_timestamp: Some(1234),
                    },
                "CHF".to_string() =>
                    QueriedExchangeRate {
                        base_asset: Asset {
                            symbol: "CHF".to_string(),
                            class: AssetClass::FiatCurrency,
                        },
                        quote_asset: Asset {
                            symbol: USD.to_string(),
                            class: AssetClass::FiatCurrency,
                        },
                        timestamp: 1234,
                        rates: vec![1_000_000_000],
                        base_asset_num_queried_sources: 5,
                        base_asset_num_received_rates: 5,
                        quote_asset_num_queried_sources: 5,
                        quote_asset_num_received_rates: 5,
                        forex_timestamp: Some(1234),
                    },
            },
        );

        assert!(matches!(
            store.get(1234, 1234, "EUR", USD),
            Ok(rate) if rate.rates == vec![1_000_000_000] && rate.base_asset_num_received_rates == 5,
        ));
        assert!(matches!(
            store.get(1234, 1234, "SGD", USD),
            Ok(rate) if rate.rates == vec![1_000_000_000] && rate.base_asset_num_received_rates == 5,
        ));
        assert!(matches!(
            store.get(1234, 1234, "CHF", USD),
            Ok(rate) if rate.rates == vec![1_000_000_000] && rate.base_asset_num_received_rates == 5,
        ));
        assert!(matches!(
            store.get(1234, 1234, "GBP", USD),
            Ok(rate) if rate.rates == vec![1_000_000_000] && rate.base_asset_num_received_rates == 2,
        ));

        assert!(matches!(
            store.get(1234, 1234, USD, "CAD"),
            Ok(rate) if rate.rates == vec![400_000_000] && rate.base_asset_num_received_rates == 2,
        ));

        assert!(matches!(
            store.get(1234, 1234, "CHF", "EUR"),
            Ok(rate) if rate.rates == vec![1_000_000_000] && rate.base_asset_num_received_rates == 5 && rate.base_asset.symbol == "CHF" && rate.quote_asset.symbol == "EUR",
        ));

        let result = store.get(1234, 1234, "HKD", USD);
        assert!(
            matches!(result, Err(GetForexRateError::CouldNotFindBaseAsset(timestamp, ref asset)) if timestamp == (1234 / SECONDS_PER_DAY) * SECONDS_PER_DAY && asset == "HKD"),
            "Expected `Err(GetForexRateError::CouldNotFindBaseAsset)`, Got: {:?}",
            result
        );
    }

    #[test]
    fn rate_store_gets_rate_in_past_if_current_day_is_not_over() {
        // Create a store, update, check that only rates with more sources were updated.
        let mut store = ForexRateStore::new();
        // Day 0
        store.put(
            0,
            hashmap! {
                "EUR".to_string() =>
                    QueriedExchangeRate {
                        base_asset: Asset {
                            symbol: "EUR".to_string(),
                            class: AssetClass::FiatCurrency,
                        },
                        quote_asset: Asset {
                            symbol: USD.to_string(),
                            class: AssetClass::FiatCurrency,
                        },
                        timestamp: 0,
                        rates: vec![800_000_000],
                        base_asset_num_queried_sources: 4,
                        base_asset_num_received_rates: 4,
                        quote_asset_num_queried_sources: 4,
                        quote_asset_num_received_rates: 4,
                        forex_timestamp: Some(0),
                    },
            },
        );
        // Day 1
        store.put(
            SECONDS_PER_DAY,
            hashmap! {
                "EUR".to_string() =>
                    QueriedExchangeRate {
                        base_asset: Asset {
                            symbol: "EUR".to_string(),
                            class: AssetClass::FiatCurrency,
                        },
                        quote_asset: Asset {
                            symbol: USD.to_string(),
                            class: AssetClass::FiatCurrency,
                        },
                        timestamp: SECONDS_PER_DAY,
                        rates: vec![1_000_000_000],
                        base_asset_num_queried_sources: 5,
                        base_asset_num_received_rates: 5,
                        quote_asset_num_queried_sources: 5,
                        quote_asset_num_received_rates: 5,
                        forex_timestamp: Some(SECONDS_PER_DAY),
                    },
            },
        );
        // Day 2
        store.put(
            SECONDS_PER_DAY * 2,
            hashmap! {
                "EUR".to_string() =>
                    QueriedExchangeRate {
                        base_asset: Asset {
                            symbol: "EUR".to_string(),
                            class: AssetClass::FiatCurrency,
                        },
                        quote_asset: Asset {
                            symbol: USD.to_string(),
                            class: AssetClass::FiatCurrency,
                        },
                        timestamp: SECONDS_PER_DAY * 2,
                        rates: vec![1_500_000_000],
                        base_asset_num_queried_sources: 5,
                        base_asset_num_received_rates: 5,
                        quote_asset_num_queried_sources: 5,
                        quote_asset_num_received_rates: 5,
                        forex_timestamp: Some(SECONDS_PER_DAY * 2),
                    },
            },
        );

        // If the current timestamp is day 1 and the requested timestamp is day 0,
        // return the timestamp for day 0.
        let result = store.get(SECONDS_PER_DAY / 2, SECONDS_PER_DAY, "EUR", USD);
        assert!(matches!(
            result,
            Ok(rate) if rate.rates == vec![800_000_000] && rate.base_asset_num_received_rates == 4,
        ));

        // If the current timestamp is 12pm UTC on day 2 and the requested timestamp is at day 1,
        // return the timestamp for day 1.
        let result = store.get(
            SECONDS_PER_DAY,
            SECONDS_PER_DAY * 2 + SECONDS_PER_DAY / 2,
            "EUR",
            USD,
        );
        assert!(matches!(
            result,
            Ok(rate) if rate.rates == vec![1_000_000_000] && rate.base_asset_num_received_rates == 5,
        ));

        // If the current timestamp is 12pm UTC on day 2 and the requested timestamp is at day 2,
        // return the rate for day 1 as day 2 is still active for some sources.
        let result = store.get(
            SECONDS_PER_DAY * 2,
            SECONDS_PER_DAY * 2 + SECONDS_PER_DAY / 2,
            "EUR",
            USD,
        );
        assert!(matches!(
            result,
            Ok(rate) if rate.rates == vec![1_000_000_000] && rate.base_asset_num_received_rates == 5,
        ));

        // If the current timestamp is 12am UTC-12 of day 3 (12am UTC+12 of day 4, means day 2 is just over anywhere on Earth)
        // and the requested timestamp is day 2, retrieve the rate at day 2.
        let result = store.get(SECONDS_PER_DAY * 2, SECONDS_PER_DAY * 4, "EUR", USD);
        assert!(matches!(
            result,
            Ok(rate) if rate.rates == vec![1_500_000_000] && rate.base_asset_num_received_rates == 5,
        ));

        // Check that `get` goes back in time to find a rate in the past.
        let result = store.get(
            SECONDS_PER_DAY * 3,
            SECONDS_PER_DAY * 3 + SECONDS_PER_DAY / 2,
            "EUR",
            USD,
        );
        assert!(matches!(
            result,
            Ok(rate) if rate.rates == vec![1_500_000_000] && rate.base_asset_num_received_rates == 5,
        ));
    }

    #[test]
    fn rate_store_get_same_asset() {
        let store = ForexRateStore::new();
        let result: Result<ExchangeRate, GetForexRateError> =
            store.get(1234, 1234, USD, USD).map(|v| {
                v.try_into()
                    .expect("The conversion to the ExchangeRate struct should work.")
            });
        assert!(matches!(result, Ok(forex_rate) if forex_rate.rate == RATE_UNIT));
        let result: Result<ExchangeRate, GetForexRateError> =
            store.get(1234, 1234, "CHF", "CHF").map(|v| {
                v.try_into()
                    .expect("The conversion to the ExchangeRate struct should work.")
            });
        assert!(matches!(result, Ok(forex_rate) if forex_rate.rate == RATE_UNIT));
    }

    /// Test that SDR and XDR rates are reported as the same asset under the symbol "xdr"
    #[test]
    fn collector_sdr_xdr() {
        let mut collector = OneDayRatesCollector {
            rates: HashMap::new(),
            timestamp: 1234,
            sources: HashSet::new(),
        };

        let rates = vec![
            ("SDR".to_string(), 1_000_000_000),
            ("XDR".to_string(), 700_000_000),
        ]
        .into_iter()
        .collect();
        collector.update("src1".to_string(), rates);

        let rates = vec![("SDR".to_string(), 1_100_000_000)]
            .into_iter()
            .collect();
        collector.update("src2".to_string(), rates);

        let rates = vec![
            ("SDR".to_string(), 1_050_000_000),
            ("XDR".to_string(), 900_000_000),
        ]
        .into_iter()
        .collect();
        collector.update("src3".to_string(), rates);

        let result: ExchangeRate = (&collector.get_rates_map()["XDR"])
            .clone()
            .try_into()
            .expect("The conversion to the ExchangeRate struct should work.");

        assert!(matches!(
            result,
            rate if rate.rate == RATE_UNIT && rate.metadata.base_asset_num_received_rates == 5,
        ))
    }

    /// This function tests that the [ForexRatesCollector] computes and adds the correct CXDR rate if
    /// all EUR/USD, CNY/USD, JPY/USD, and GBP/USD rates are available.
    #[test]
    fn verify_compute_xdr_rate() {
        let mut map: HashMap<String, Vec<u64>> = HashMap::new();
        map.insert(
            "EUR".to_string(),
            vec![979_500_000, 981_500_000, 969_800_000],
        ); // median: 979_500_000
        map.insert("CNY".to_string(), vec![140_500_000, 148_900_000]); // median: 144_700_000
        map.insert(
            "JPY".to_string(),
            vec![6_900_000, 7_100_000, 6_800_000, 7_000_000],
        ); // median: 6_950_000
        map.insert(
            "GBP".to_string(),
            vec![1_121_200_000, 1_122_000_000, 1_120_900_000],
        ); // median: 1_121_200_000

        let collector = OneDayRatesCollector {
            rates: map,
            timestamp: 0,
            sources: HashSet::new(),
        };

        let rates_map = collector.get_rates_map();
        let cxdr_usd_rate: ExchangeRate = rates_map
            .get(COMPUTED_XDR_SYMBOL)
            .expect("A rate should be returned")
            .clone()
            .try_into()
            .expect("The conversion to the ExchangeRate struct should work.");

        // The expected CXDR/USD rate is
        // 0.58252*1.0+0.38671*0.9795+1.0174*0.1447+11.9*0.00695+0.085946*1.1212
        // = 1.28758788

        // The expected variance is
        // EUR_XDR_WEIGHT^2*Var(EUR) + CNY_XDR_WEIGHT^2*Var(CNY)
        // + JPY_XDR_WEIGHT^2*Var(JPY) + GBP_XDR_WEIGHT^2*Var(GBP) or, equivalently
        // (EUR_XDR_WEIGHT*std_dev(EUR))^2 + (CNY_XDR_WEIGHT*std_dev(CNY))^2
        // + (JPY_XDR_WEIGHT*std_dev(JPY))^2 + (GBP_XDR_WEIGHT*std_dev(GBP))^2, which is
        // (0.386710*0.006258061)^2 + (1.0174*0.005939696)^2 + (11.9*0.000129099)^2 + (0.085946*0.000568624)^2
        // = 0.006688618.
        // The standard deviation is sqrt(0.000044738) = 0.00065.

        let _expected_rate = ExchangeRate {
            base_asset: Asset {
                symbol: "CXDR".to_string(),
                class: AssetClass::FiatCurrency,
            },
            quote_asset: Asset {
                symbol: USD.to_string(),
                class: AssetClass::FiatCurrency,
            },
            timestamp: 0,
            rate: 1287587880,
            metadata: ExchangeRateMetadata {
                decimals: DECIMALS,
                base_asset_num_queried_sources: FOREX_SOURCES.len(),
                base_asset_num_received_rates: 2,
                quote_asset_num_queried_sources: FOREX_SOURCES.len(),
                quote_asset_num_received_rates: 2,
                standard_deviation: 6688618,
                forex_timestamp: Some(0),
            },
        };

        assert_eq!(cxdr_usd_rate, _expected_rate);
    }

    /// This function tests that the computed set of artificial CXDR rates does not contain any zero rates.
    /// The fiat currency rates are taken from a real execution, which caused a CXDR rate to be
    /// zero because of a wrong JPY rate.
    #[test]
    fn no_zero_rates_when_computing_xdr_rate() {
        let mut map: HashMap<String, Vec<u64>> = HashMap::new();
        map.insert(
            "EUR".to_string(),
            vec![1_064_600_059, 1_066_000_000, 1_066_500_000, 1_069_086_041],
        );
        map.insert(
            "CNY".to_string(),
            vec![144_121_365, 144_170_327, 144_250_605, 144_266_666],
        );
        // The JPY rates contain an entry that is hundred times too large.
        map.insert(
            "JPY".to_string(),
            vec![7_344_535, 7_354_056, 7_360_340, 736_571_428],
        );
        map.insert(
            "GBP".to_string(),
            vec![1_198_745_616, 1_201_173_556, 1_201_190_476, 1_204_432_215],
        );

        let collector = OneDayRatesCollector {
            rates: map,
            timestamp: 0,
            sources: HashSet::new(),
        };

        let rates_map = collector.get_rates_map();
        let cxdr_usd_rate = rates_map
            .get(COMPUTED_XDR_SYMBOL)
            .expect("A rate should be returned");

        assert_ne!(cxdr_usd_rate.rates[0], 0);
    }

    /// Test transform_http_response_body to the correct set of bytes.
    #[test]
    fn encoding_transformed_http_response() {
        let forex = Forex::EuropeanCentralBank(EuropeanCentralBank);
        let body = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<gesmes:Envelope xmlns:gesmes=\"http://www.gesmes.org/xml/2002-08-01\" xmlns=\"http://www.ecb.int/vocabulary/2002-08-01/eurofxref\">\n    <gesmes:subject>Reference rates</gesmes:subject>\n    <gesmes:Sender>\n        <gesmes:name>European Central Bank</gesmes:name>\n    </gesmes:Sender>\n    <Cube>\n        <Cube time='2022-10-03'>\n            <Cube currency='USD' rate='0.9764' />\n            <Cube currency='JPY' rate='141.49' />\n            <Cube currency='BGN' rate='1.9558' />\n            <Cube currency='CZK' rate='24.527' />\n            <Cube currency='DKK' rate='7.4366' />\n            <Cube currency='GBP' rate='0.87070' />\n            <Cube currency='HUF' rate='424.86' />\n            <Cube currency='PLN' rate='4.8320' />\n            <Cube currency='RON' rate='4.9479' />\n            <Cube currency='SEK' rate='10.8743' />\n            <Cube currency='CHF' rate='0.9658' />\n            <Cube currency='ISK' rate='141.70' />\n            <Cube currency='NOK' rate='10.5655' />\n            <Cube currency='HRK' rate='7.5275' />\n            <Cube currency='TRY' rate='18.1240' />\n            <Cube currency='AUD' rate='1.5128' />\n            <Cube currency='BRL' rate='5.1780' />\n            <Cube currency='CAD' rate='1.3412' />\n            <Cube currency='CNY' rate='6.9481' />\n            <Cube currency='HKD' rate='7.6647' />\n            <Cube currency='IDR' rate='14969.79' />\n            <Cube currency='ILS' rate='3.4980' />\n            <Cube currency='INR' rate='79.8980' />\n            <Cube currency='KRW' rate='1408.25' />\n            <Cube currency='MXN' rate='19.6040' />\n            <Cube currency='MYR' rate='4.5383' />\n            <Cube currency='NZD' rate='1.7263' />\n            <Cube currency='PHP' rate='57.599' />\n            <Cube currency='SGD' rate='1.4015' />\n            <Cube currency='THB' rate='37.181' />\n            <Cube currency='ZAR' rate='17.5871' />\n        </Cube>\n    </Cube>\n</gesmes:Envelope>".as_bytes();
        let context_bytes = forex
            .encode_context(&ForexContextArgs {
                timestamp: 1664755200,
            })
            .expect("should be able to encode");
        let context =
            Forex::decode_context(&context_bytes).expect("should be able to decode bytes");
        let bytes = forex
            .transform_http_response_body(body, &context.payload)
            .expect("should be able to transform the body");
        let result = Forex::decode_response(&bytes);

        assert!(matches!(result, Ok(map) if map["EUR"] == 976_400_000));
    }

    /// Test that response decoding works correctly.
    #[test]
    fn decode_transformed_http_response() {
        let hex_string = "4449444c026d016c0200710178010001034555520100000000000000";
        let bytes = hex::decode(hex_string).expect("should be able to decode");
        let result = Forex::decode_response(&bytes);
        assert!(matches!(result, Ok(map) if map["EUR"] == 1));
    }

    /// This function tests that the [ForexRateStore] can return the amount of bytes it has
    /// allocated over time.
    #[test]
    fn forex_rate_store_can_return_the_number_of_bytes_allocated_to_it() {
        let mut store = ForexRateStore::new();

        store.put(
            0,
            hashmap! {
                "EUR".to_string() => QueriedExchangeRate {
                    base_asset: Asset {
                        symbol: "EUR".to_string(),
                        class: AssetClass::FiatCurrency,
                    },
                    quote_asset: Asset {
                        symbol: USD.to_string(),
                        class: AssetClass::FiatCurrency,
                    },
                    timestamp: 1234,
                    rates: vec![10_000],
                    base_asset_num_queried_sources: 5,
                    base_asset_num_received_rates: 5,
                    quote_asset_num_queried_sources: 5,
                    quote_asset_num_received_rates: 5,
                    forex_timestamp: Some(1234),
                }
            },
        );

        assert_eq!(store.allocated_bytes(), 273);
    }

    /// This function tests the "go back" mechanism where, when there are no rates for a requested timestamp, we may go back up to [MAX_DAYS_TO_GO_BACK] days.
    #[test]
    fn forex_go_back_days() {
        let mut store = ForexRateStore::new();

        let timestamp = 1661990400; // Corresponds to 2022-09-01
        let queried_timestamp = timestamp + SECONDS_PER_DAY * MAX_DAYS_TO_GO_BACK;

        store.put(
            timestamp,
            hashmap! {
                "EUR".to_string() => QueriedExchangeRate {
                    base_asset: Asset {
                        symbol: "EUR".to_string(),
                        class: AssetClass::FiatCurrency,
                    },
                    quote_asset: Asset {
                        symbol: USD.to_string(),
                        class: AssetClass::FiatCurrency,
                    },
                    timestamp,
                    rates: vec![10_000],
                    base_asset_num_queried_sources: 5,
                    base_asset_num_received_rates: 5,
                    quote_asset_num_queried_sources: 5,
                    quote_asset_num_received_rates: 5,
                    forex_timestamp: Some(timestamp),
                }
            },
        );

        // Assert that we can retrieve rates up to [MAX_DAYS_TO_GO_BACK] days back.
        assert_eq!(
            store
                .get(queried_timestamp, queried_timestamp, "EUR", USD)
                .unwrap()
                .forex_timestamp
                .unwrap(),
            timestamp
        );
        // But also that we cannot retrieve rates for more than that.
        let queried_timestamp = queried_timestamp + SECONDS_PER_DAY * 2 + SECONDS_PER_DAY / 2;
        assert!(matches!(
            store.get(queried_timestamp, queried_timestamp, "EUR", USD),
            Err(GetForexRateError::InvalidTimestamp(_queried_timestamp))
        ));
    }

    /// This function tests that the forex sources can report the max response bytes needed to make a successful HTTP outcall.
    #[test]
    fn forex_max_response_bytes() {
        let forex = Forex::MonetaryAuthorityOfSingapore(MonetaryAuthorityOfSingapore);
        assert_eq!(forex.max_response_bytes(), 3 * ONE_KIB);
        let forex = Forex::CentralBankOfMyanmar(CentralBankOfMyanmar);
        assert_eq!(forex.max_response_bytes(), 3 * ONE_KIB);
        let forex = Forex::CentralBankOfBosniaHerzegovina(CentralBankOfBosniaHerzegovina);
        assert_eq!(forex.max_response_bytes(), 30 * ONE_KIB);
        let forex = Forex::EuropeanCentralBank(EuropeanCentralBank);
        assert_eq!(forex.max_response_bytes(), 3 * ONE_KIB);
        let forex = Forex::BankOfCanada(BankOfCanada);
        assert_eq!(forex.max_response_bytes(), 10 * ONE_KIB);
        let forex = Forex::CentralBankOfUzbekistan(CentralBankOfUzbekistan);
        assert_eq!(forex.max_response_bytes(), 30 * ONE_KIB);
    }

    #[test]
    #[cfg(not(feature = "ipv4-support"))]
    fn is_available() {
        let available_forex_sources_count =
            FOREX_SOURCES.iter().filter(|e| e.is_available()).count();
        assert_eq!(available_forex_sources_count, 3);
    }

    #[test]
    #[cfg(feature = "ipv4-support")]
    fn is_available_ipv4() {
        let available_forex_sources_count =
            FOREX_SOURCES.iter().filter(|e| e.is_available()).count();
        assert_eq!(available_forex_sources_count, 7);
    }

    #[test]
    fn correct_shift_to_latest_source_eod() {
        // Let the current time be day 2, noon UTC
        let current_timestamp = SECONDS_PER_DAY * 2 + SECONDS_PER_DAY / 2;
        // Try the timestamp of the beginning of day 2 UTC
        // Expect a shift to day 1
        let requested_timestamp = SECONDS_PER_DAY * 2;
        let shifted =
            ForexRateStore::shift_to_latest_source_eod(requested_timestamp, current_timestamp);
        assert_eq!(shifted, SECONDS_PER_DAY);
        // Try the timestamp of the beginning of day 1 UTC
        // Expect no shift
        let requested_timestamp = SECONDS_PER_DAY;
        let shifted =
            ForexRateStore::shift_to_latest_source_eod(requested_timestamp, current_timestamp);
        assert_eq!(shifted, requested_timestamp);
    }
}
