use std::collections::BTreeSet;

use candid::{decode_args, encode_args, Deserialize, Error as CandidError};

use ic_xrc_types::Asset;
use serde::de::DeserializeOwned;

use crate::api::usd_asset;
use crate::{usdt_asset, utils, ONE_KIB};
use crate::{ExtractError, RATE_UNIT};
use crate::{USDC, USDS, USDT};

/// This macro generates the necessary boilerplate when adding an exchange to this module.
macro_rules! exchanges {
    ($($name:ident),*) => {
        /// Enum that contains all of the supported cryptocurrency exchanges.
        #[derive(PartialEq, Clone, Debug)]
        pub enum Exchange {
            $(
                #[allow(missing_docs)]
                $name($name),
            )*
        }

        $(
            #[derive(PartialEq, Clone, Debug)]
            pub struct $name;
        )*

        impl core::fmt::Display for Exchange {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.name())
            }
        }

        /// Contains all of the known exchanges that can be found in the
        /// [Exchange] enum.
        pub const EXCHANGES: &'static [Exchange] = &[
            $(Exchange::$name($name)),*
        ];


        /// Implements the core functionality of the generated `Exchange` enum.
        impl Exchange {

            /// Returns the exchange's canonical name as a static string —
            /// the same text that `Display` produces, but without
            /// allocating. Use this on hot paths (e.g. metric-label
            /// recording) where `to_string()` would allocate once per call.
            pub fn name(&self) -> &'static str {
                match self {
                    $(Exchange::$name(_) => stringify!($name)),*,
                }
            }

            /// Retrieves the position of the exchange in the EXCHANGES array.
            pub fn get_index(&self) -> usize {
                EXCHANGES.iter().position(|e| e == self).expect("should contain the exchange")
            }

            /// This method returns the formatted URL for the exchange.
            pub fn get_url(&self, base_asset: &str, quote_asset: &str, timestamp: u64) -> String {
                match self {
                    $(Exchange::$name(exchange) => exchange.get_url(base_asset, quote_asset, timestamp)),*,
                }
            }

            /// This method extracts the rate encoded in the given input.
            pub fn extract_rate(&self, bytes: &[u8]) -> Result<u64, ExtractError> {
                match self {
                    $(Exchange::$name(exchange) => exchange.extract_rate(bytes)),*,
                }
            }

            /// This method returns the URL of the exchange's public spot-listing
            /// endpoint (used to discover tradable pairs).
            pub fn listing_url(&self) -> &str {
                match self {
                    $(Exchange::$name(exchange) => exchange.listing_url()),*,
                }
            }

            /// This method parses a listing-endpoint response into the set of base
            /// assets the exchange currently lists against USDT, plus the total
            /// number of spot markets parsed (see [ListedPairs]).
            pub fn extract_listed_usdt_bases(&self, bytes: &[u8]) -> Result<ListedPairs, ExtractError> {
                match self {
                    $(Exchange::$name(exchange) => exchange.extract_listed_usdt_bases(bytes)),*,
                }
            }

            /// This method checks if the exchange supports IPv6.
            pub fn supports_ipv6(&self) -> bool {
                match self {
                    $(Exchange::$name(exchange) => exchange.supports_ipv6()),*,
                }
            }

            /// This method lists the USD assets supported by the exchange.
            pub fn supported_usd_asset_type(&self) -> Asset {
                match self {
                    $(Exchange::$name(exchange) => exchange.supported_usd_asset()),*,
                }
            }

            /// This method lists the supported stablecoin pairs of the exchange.
            pub fn supported_stablecoin_pairs(&self) -> &[(&str, &str)] {
                match self {
                    $(Exchange::$name(exchange) => exchange.supported_stablecoin_pairs()),*,
                }
            }

            /// Encodes the context in relation to the current exchange.
            pub fn encode_context(&self) -> Result<Vec<u8>, CandidError> {
                let index = self.get_index();
                encode_args((index,))
            }

            /// A general method to decode contexts from an `Exchange`.
            pub fn decode_context(bytes: &[u8]) -> Result<usize, CandidError> {
                decode_args::<(usize,)>(bytes).map(|decoded| decoded.0)
            }

            /// Encodes the response in the exchange transform method.
            pub fn encode_response(rate: u64) -> Result<Vec<u8>, CandidError> {
                encode_args((rate,))
            }

            /// Decodes the response from the exchange transform method.
            pub fn decode_response(bytes: &[u8]) -> Result<u64, CandidError> {
                decode_args::<(u64,)>(bytes).map(|decoded| decoded.0)
            }

            /// Encodes a parsed listing as the listing transform's output — the
            /// small, canonical payload the replicas reach consensus on. The
            /// bases are a `BTreeSet`, so candid emits them in a deterministic
            /// (sorted) order.
            pub fn encode_listing_response(listed: &ListedPairs) -> Result<Vec<u8>, CandidError> {
                encode_args((&listed.bases, listed.total_markets as u64))
            }

            /// Decodes the listing payload produced by [encode_listing_response].
            pub fn decode_listing_response(bytes: &[u8]) -> Result<ListedPairs, CandidError> {
                decode_args::<(BTreeSet<String>, u64)>(bytes).map(|(bases, total_markets)| {
                    ListedPairs {
                        bases,
                        total_markets: total_markets as usize,
                    }
                })
            }

            /// This method returns the exchange's max response bytes.
            pub fn max_response_bytes(&self) -> u64 {
                match self {
                    $(Exchange::$name(exchange) => exchange.max_response_bytes()),*,
                }
            }

            /// This method returns the exchange's max response bytes for a
            /// listing outcall.
            pub fn listing_max_response_bytes(&self) -> u64 {
                match self {
                    $(Exchange::$name(exchange) => exchange.listing_max_response_bytes()),*,
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

            /// This method returns the number of cycles expected to be sent when
            /// calling an exchange. The value returned is at least the maximum
            /// required for each exchanges.
            pub fn cycles(&self) -> u128 {
                if cfg!(feature = "application-subnet") {
                    500_000_000
                } else {
                    0
                }
            }
        }
    }

}

exchanges! { Coinbase, KuCoin, Okx, GateIo, Mexc, Poloniex, CryptoCom, Bitget, Digifinex }

/// Used to determine how to parse the extracted value returned from
/// [extract_rate]'s `extract_fn` argument.
enum ExtractedValue {
    Str(String),
    Float(f64),
}

/// This function provides a generic way to extract a rate out of the provided bytes.
fn extract_rate<R: DeserializeOwned>(
    bytes: &[u8],
    extract_fn: impl FnOnce(R) -> Option<ExtractedValue>,
) -> Result<u64, ExtractError> {
    let response = serde_json::from_slice::<R>(bytes)
        .map_err(|err| ExtractError::json_deserialize(bytes, err.to_string()))?;
    let extracted_value = extract_fn(response).ok_or_else(|| ExtractError::extract(bytes))?;
    let rate = match extracted_value {
        ExtractedValue::Str(value) => value
            .parse::<f64>()
            .map_err(|err| ExtractError::json_deserialize(bytes, err.to_string()))?,
        ExtractedValue::Float(value) => value,
    };

    Ok((rate * RATE_UNIT as f64) as u64)
}

/// A single spot market parsed from an exchange's listing endpoint, normalized
/// across the differing per-exchange schemas.
struct ListedMarket {
    /// The base asset symbol, as reported by the exchange.
    base: String,
    /// The quote asset symbol, as reported by the exchange.
    quote: String,
    /// Whether the market is currently tradable (per the exchange's own status
    /// field; the rule differs per exchange).
    tradable: bool,
}

impl ListedMarket {
    fn new(base: impl Into<String>, quote: impl Into<String>, tradable: bool) -> Self {
        Self {
            base: base.into(),
            quote: quote.into(),
            tradable,
        }
    }
}

/// The result of parsing an exchange's listing endpoint:
/// * `bases` — the base assets currently tradable against USDT, uppercased and
///   deduplicated. This is the set the crypto path is gated on.
/// * `total_markets` — the total number of spot markets parsed across all
///   quotes. This is a structural-health signal for the refresh acceptance
///   guard: it stays roughly stable across refreshes (even when a venue
///   migrates USDT→USD, collapsing `bases`), so a sudden drop indicates a
///   parser break or a garbage response rather than a legitimate delisting.
///   Caveat: MEXC's `defaultSymbols` endpoint enumerates only *tradable*
///   symbols (see the MEXC impl), so there `total_markets` is weaker as a
///   stability signal — a mass trading suspension would dent it even though the
///   response is structurally sound.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListedPairs {
    pub bases: BTreeSet<String>,
    pub total_markets: usize,
}

/// A generic way to extract the listed USDT bases out of the provided bytes,
/// mirroring [extract_rate]: `markets_fn` projects the deserialized response
/// down to the exchange's spot markets, then this folds over them in a single
/// pass — keeping the USDT-quoted tradable bases and counting every market.
///
/// Taking an [IntoIterator] (rather than a materialized `Vec`) lets each
/// exchange stream its markets straight through without an intermediate
/// allocation, while the USDT/tradable filter, base uppercasing, and
/// total-market count stay defined in this one place for all exchanges.
fn extract_listed_pairs<R, M>(
    bytes: &[u8],
    markets_fn: impl FnOnce(R) -> M,
) -> Result<ListedPairs, ExtractError>
where
    R: DeserializeOwned,
    M: IntoIterator<Item = ListedMarket>,
{
    let response = serde_json::from_slice::<R>(bytes)
        .map_err(|err| ExtractError::json_deserialize(bytes, err.to_string()))?;
    let mut total_markets = 0;
    let mut bases = BTreeSet::new();
    for market in markets_fn(response) {
        total_markets += 1;
        if market.tradable && market.quote.eq_ignore_ascii_case(USDT) {
            bases.insert(market.base.to_uppercase());
        }
    }
    Ok(ListedPairs {
        bases,
        total_markets,
    })
}

/// The base URL may contain the following placeholders:
/// `BASE_ASSET`: This string must be replaced with the base asset string in the request.
const BASE_ASSET: &str = "BASE_ASSET";
/// `QUOTE_ASSET`: This string must be replaced with the quote asset string in the request.
const QUOTE_ASSET: &str = "QUOTE_ASSET";
/// `START_TIME`: This string must be replaced with the start time derived from the timestamp in the request.
const START_TIME: &str = "START_TIME";
/// `END_TIME`: This string must be replaced with the end time derived from the timestamp in the request.
const END_TIME: &str = "END_TIME";

/// Default cap on the raw listing response a refresh outcall will accept. The
/// XRC's largest listing (OKX, ~1.3 MiB) fits with headroom under the IC's
/// ~2 MiB HTTP-outcall limit, and the subnet is feeless so over-provisioning
/// costs nothing; a per-exchange override can tighten or loosen it if a venue's
/// listing grows.
const DEFAULT_LISTING_MAX_RESPONSE_BYTES: u64 = 1_900_000;

/// This trait is use to provide the basic methods needed for an exchange.
trait IsExchange {
    /// The base URL template that is provided to [IsExchange::get_url].
    fn get_base_url(&self) -> &str;

    /// Provides the ability to format an asset code. Default implementation is
    /// to return the code as uppercase.
    fn format_asset(&self, asset: &str) -> String {
        asset.to_uppercase()
    }

    /// Provides the ability to format the start time. Default implementation is
    /// to simply return the provided timestamp as a string.
    fn format_start_time(&self, timestamp: u64) -> String {
        timestamp.to_string()
    }

    /// Provides the ability to format the end time. Default implementation is
    /// to simply return the provided timestamp as a string.
    fn format_end_time(&self, timestamp: u64) -> String {
        timestamp.to_string()
    }

    /// A default implementation to generate a URL based on the given parameters.
    /// The method takes the base URL for the exchange and replaces the following
    /// placeholders:
    /// * [BASE_ASSET]
    /// * [QUOTE_ASSET]
    /// * [START_TIME]
    /// * [END_TIME]
    fn get_url(&self, base_asset: &str, quote_asset: &str, timestamp: u64) -> String {
        let timestamp = (timestamp / 60) * 60;
        self.get_base_url()
            .replace(BASE_ASSET, &self.format_asset(base_asset))
            .replace(QUOTE_ASSET, &self.format_asset(quote_asset))
            .replace(START_TIME, &self.format_start_time(timestamp))
            .replace(END_TIME, &self.format_end_time(timestamp))
    }

    /// The implementation to extract the rate from the response's body.
    fn extract_rate(&self, bytes: &[u8]) -> Result<u64, ExtractError>;

    /// The URL of the exchange's public spot-listing endpoint. Unlike
    /// [IsExchange::get_base_url] this takes no placeholders — the listing is
    /// the same for every asset.
    fn listing_url(&self) -> &str;

    /// Parses the listing endpoint's response body into the set of base assets
    /// tradable against USDT and the total spot-market count (see
    /// [ListedPairs]). Implementations project their own schema to a list of
    /// [ListedMarket] and delegate to [extract_listed_pairs].
    fn extract_listed_usdt_bases(&self, bytes: &[u8]) -> Result<ListedPairs, ExtractError>;

    /// Indicates if the exchange supports IPv6.
    fn supports_ipv6(&self) -> bool {
        false
    }

    /// Return the exchange's supported USD asset type.
    fn supported_usd_asset(&self) -> Asset {
        usdt_asset()
    }

    fn supported_stablecoin_pairs(&self) -> &[(&str, &str)] {
        &[(USDS, USDT), (USDC, USDT)]
    }

    fn max_response_bytes(&self) -> u64 {
        3 * ONE_KIB
    }

    /// The max response size for this exchange's listing outcall. Listings are
    /// far larger than rate responses, so this overrides [max_response_bytes].
    fn listing_max_response_bytes(&self) -> u64 {
        DEFAULT_LISTING_MAX_RESPONSE_BYTES
    }
}

/// Coinbase
type CoinbaseResponse = Vec<(u64, f64, f64, f64, f64, f64)>;

/// Coinbase differs from most other exchanges in two ways that make a
/// single-minute `start == end` query unreliable:
///   1. It returns no candle at all for a minute in which no trade occurred
///      (it does not forward-fill), so the response is an empty `[]`.
///   2. It will return the still-forming current minute, whose value keeps
///      changing as trades arrive; replicas making the HTTP outcall at slightly
///      different instants then see different bytes and fail to reach consensus.
///
/// `transform_exchange_http_response` records both cases as a failed outcall,
/// which is why Coinbase shows a high `http_error` rate in production even
/// though it is a high-volume exchange.
///
/// To avoid both, we request a short window of already-closed minutes ending
/// one minute before the requested timestamp and rely on Coinbase returning
/// candles newest-first, so `extract_rate`'s `.first()` yields the most recent
/// closed candle at or before the timestamp. This is the same "most recent
/// candle <= timestamp" behaviour OKX and Bitget already rely on, which the
/// downstream pipeline tolerates (the extractor never matches the candle's own
/// timestamp against the request). The window stays well within
/// `max_response_bytes` (at most a handful of candles).
const COINBASE_CANDLE_END_OFFSET_SEC: u64 = 60;
const COINBASE_CANDLE_LOOKBACK_SEC: u64 = 6 * 60;

/// A single entry from Coinbase's `/products` listing (only the fields needed
/// to decide tradability against USDT).
#[derive(Deserialize)]
struct CoinbaseProduct {
    base_currency: String,
    quote_currency: String,
    status: String,
    #[serde(default)]
    trading_disabled: bool,
}
type CoinbaseListing = Vec<CoinbaseProduct>;

impl IsExchange for Coinbase {
    fn get_base_url(&self) -> &str {
        "https://api.exchange.coinbase.com/products/BASE_ASSET-QUOTE_ASSET/candles?granularity=60&start=START_TIME&end=END_TIME"
    }

    fn format_start_time(&self, timestamp: u64) -> String {
        timestamp
            .saturating_sub(COINBASE_CANDLE_LOOKBACK_SEC)
            .to_string()
    }

    fn format_end_time(&self, timestamp: u64) -> String {
        // End one minute before the requested timestamp so the newest candle in
        // range is an already-closed minute, never the still-forming current
        // one (see the note above).
        timestamp
            .saturating_sub(COINBASE_CANDLE_END_OFFSET_SEC)
            .to_string()
    }

    fn extract_rate(&self, bytes: &[u8]) -> Result<u64, ExtractError> {
        extract_rate(bytes, |response: CoinbaseResponse| {
            response.first().map(|kline| ExtractedValue::Float(kline.3))
        })
    }

    fn listing_url(&self) -> &str {
        "https://api.exchange.coinbase.com/products"
    }

    fn extract_listed_usdt_bases(&self, bytes: &[u8]) -> Result<ListedPairs, ExtractError> {
        extract_listed_pairs(bytes, |products: CoinbaseListing| {
            products.into_iter().map(|product| {
                let tradable = product.status == "online" && !product.trading_disabled;
                ListedMarket::new(product.base_currency, product.quote_currency, tradable)
            })
        })
    }

    fn supports_ipv6(&self) -> bool {
        true
    }

    fn supported_usd_asset(&self) -> Asset {
        usd_asset()
    }

    fn supported_stablecoin_pairs(&self) -> &[(&str, &str)] {
        &[(USDT, USDC)]
    }
}

/// KuCoin
#[derive(Deserialize)]
struct KuCoinResponse {
    data: Vec<(String, String, String, String, String, String, String)>,
}

/// Like Coinbase, KuCoin returns the still-forming current minute for a single
/// `startAt == endAt` query, so replicas making the HTTP outcall at slightly
/// different instants see different bytes and fail to reach consensus (recorded
/// as a failed outcall). To avoid this we request a short window of
/// already-closed minutes ending one minute before the requested timestamp.
/// KuCoin returns candles newest-first, so `extract_rate`'s `.first()` yields
/// the most recent closed candle at or before the timestamp. The window stays
/// well within `max_response_bytes` (at most a handful of candles).
const KUCOIN_CANDLE_END_OFFSET_SEC: u64 = 60;
const KUCOIN_CANDLE_LOOKBACK_SEC: u64 = 6 * 60;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KuCoinSymbol {
    base_currency: String,
    quote_currency: String,
    enable_trading: bool,
}
#[derive(Deserialize)]
struct KuCoinSymbolsResponse {
    data: Vec<KuCoinSymbol>,
}

impl IsExchange for KuCoin {
    fn get_base_url(&self) -> &str {
        "https://api.kucoin.com/api/v1/market/candles?symbol=BASE_ASSET-QUOTE_ASSET&type=1min&startAt=START_TIME&endAt=END_TIME"
    }

    fn format_start_time(&self, timestamp: u64) -> String {
        timestamp
            .saturating_sub(KUCOIN_CANDLE_LOOKBACK_SEC)
            .to_string()
    }

    fn format_end_time(&self, timestamp: u64) -> String {
        // End one minute before the requested timestamp so the newest in-range
        // candle is an already-closed minute, never the still-forming current
        // one. The extra second is kept because KuCoin needs `endAt` past the
        // candle's start second to include it.
        timestamp
            .saturating_sub(KUCOIN_CANDLE_END_OFFSET_SEC)
            .saturating_add(1)
            .to_string()
    }

    fn extract_rate(&self, bytes: &[u8]) -> Result<u64, ExtractError> {
        extract_rate(bytes, |response: KuCoinResponse| {
            response
                .data
                .first()
                .map(|kline| ExtractedValue::Str(kline.1.clone()))
        })
    }

    fn listing_url(&self) -> &str {
        "https://api.kucoin.com/api/v1/symbols"
    }

    fn extract_listed_usdt_bases(&self, bytes: &[u8]) -> Result<ListedPairs, ExtractError> {
        extract_listed_pairs(bytes, |response: KuCoinSymbolsResponse| {
            response.data.into_iter().map(|symbol| {
                ListedMarket::new(
                    symbol.base_currency,
                    symbol.quote_currency,
                    symbol.enable_trading,
                )
            })
        })
    }

    fn supports_ipv6(&self) -> bool {
        true
    }

    // Drop USDS-USDT from the default pair set. KuCoin's USDS-USDT market is
    // near-dead (it was only listed in April 2026, trades a handful of minutes per
    // hour, and is otherwise forward-filled), so the single-minute candle query
    // returns an empty `data` array on most polls, which surfaces as a fetch
    // error. USDS-USDT is well covered by other exchanges; only the healthy
    // USDC-USDT pair is queried here.
    fn supported_stablecoin_pairs(&self) -> &[(&str, &str)] {
        &[(USDC, USDT)]
    }
}

/// OKX
/// https://www.okx.com/docs-v5/en/#rest-api-market-data-get-candlesticks
type OkxResponseDataEntry = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
);

#[derive(Deserialize)]
struct OkxResponse {
    data: Vec<OkxResponseDataEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OkxInstrument {
    base_ccy: String,
    quote_ccy: String,
    state: String,
}
#[derive(Deserialize)]
struct OkxInstrumentsResponse {
    data: Vec<OkxInstrument>,
}

impl IsExchange for Okx {
    fn get_base_url(&self) -> &str {
        // Counterintuitively, "after" specifies the end time, and "before" specifies the start time.
        "https://www.okx.com/api/v5/market/history-candles?instId=BASE_ASSET-QUOTE_ASSET&bar=1m&before=START_TIME&after=END_TIME"
    }

    fn format_start_time(&self, timestamp: u64) -> String {
        // Convert seconds to milliseconds and subtract 1 minute and 1 millisecond.
        // A minute is subtracted because OKX does not return rates for the current minute.
        // Subtracting a minute does not invalidate results when the request contains a timestamp
        // in the past because the most recent candle data is always at index 0.
        timestamp
            .saturating_mul(1000)
            .saturating_sub(60_001)
            .to_string()
    }

    fn format_end_time(&self, timestamp: u64) -> String {
        // Convert seconds to milliseconds and add 1 millisecond.
        timestamp.saturating_mul(1000).saturating_add(1).to_string()
    }

    fn extract_rate(&self, bytes: &[u8]) -> Result<u64, ExtractError> {
        extract_rate(bytes, |response: OkxResponse| {
            response
                .data
                .first()
                .map(|kline| ExtractedValue::Str(kline.1.clone()))
        })
    }

    fn listing_url(&self) -> &str {
        "https://www.okx.com/api/v5/public/instruments?instType=SPOT"
    }

    fn extract_listed_usdt_bases(&self, bytes: &[u8]) -> Result<ListedPairs, ExtractError> {
        extract_listed_pairs(bytes, |response: OkxInstrumentsResponse| {
            response.data.into_iter().map(|instrument| {
                ListedMarket::new(
                    instrument.base_ccy,
                    instrument.quote_ccy,
                    instrument.state == "live",
                )
            })
        })
    }

    fn supports_ipv6(&self) -> bool {
        true
    }
}

/// Gate.io
type GateIoResponse = Vec<(
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
)>;

#[derive(Deserialize)]
struct GateIoPair {
    base: String,
    quote: String,
    trade_status: String,
}
type GateIoListing = Vec<GateIoPair>;

impl IsExchange for GateIo {
    fn get_base_url(&self) -> &str {
        "https://api.gateio.ws/api/v4/spot/candlesticks?currency_pair=BASE_ASSET_QUOTE_ASSET&interval=1m&from=START_TIME&to=END_TIME"
    }

    fn extract_rate(&self, bytes: &[u8]) -> Result<u64, ExtractError> {
        extract_rate(bytes, |response: GateIoResponse| {
            response
                .first()
                .map(|kline| ExtractedValue::Str(kline.3.clone()))
        })
    }

    fn listing_url(&self) -> &str {
        "https://api.gateio.ws/api/v4/spot/currency_pairs"
    }

    fn extract_listed_usdt_bases(&self, bytes: &[u8]) -> Result<ListedPairs, ExtractError> {
        extract_listed_pairs(bytes, |pairs: GateIoListing| {
            pairs
                .into_iter()
                .map(|pair| ListedMarket::new(pair.base, pair.quote, pair.trade_status == "tradable"))
        })
    }
}

/// MEXC
///
/// Each element is a kline (candlestick) tuple with the following fields
/// (see <https://www.mexc.com/api-docs/spot-v3/market-data-endpoints/klinecandlestick-data>):
///
/// | Index | Description        |
/// |-------|--------------------|
/// | 0     | Open time          |
/// | 1     | Open               |
/// | 2     | High               |
/// | 3     | Low                |
/// | 4     | Close              |
/// | 5     | Volume             |
/// | 6     | Close time         |
/// | 7     | Quote asset volume |
#[allow(clippy::type_complexity)]
type MexcResponse = Vec<(u64, String, String, String, String, String, u64, String)>;

/// MEXC's `defaultSymbols` lists only tradable symbols as concatenated strings
/// (e.g. `"BTCUSDT"`) with no separator, so the quote can only be recovered for
/// the suffixes we care about. Non-USDT symbols are kept (so they count toward
/// `total_markets`) but cannot be split, so their quote is left unknown. Note
/// that because this endpoint omits non-tradable symbols, `total_markets` here
/// tracks the tradable universe rather than the full listing — a weaker
/// structural-health signal than for other exchanges (see [ListedPairs]).
#[derive(Deserialize)]
struct MexcDefaultSymbols {
    data: Vec<String>,
}

impl IsExchange for Mexc {
    fn get_base_url(&self) -> &str {
        "https://api.mexc.com/api/v3/klines?symbol=BASE_ASSETQUOTE_ASSET&interval=1m&startTime=START_TIME&limit=1"
    }

    fn extract_rate(&self, bytes: &[u8]) -> Result<u64, ExtractError> {
        extract_rate(bytes, |response: MexcResponse| {
            response
                .first()
                .map(|kline| ExtractedValue::Str(kline.1.clone()))
        })
    }

    fn listing_url(&self) -> &str {
        "https://api.mexc.com/api/v3/defaultSymbols"
    }

    fn extract_listed_usdt_bases(&self, bytes: &[u8]) -> Result<ListedPairs, ExtractError> {
        extract_listed_pairs(bytes, |response: MexcDefaultSymbols| {
            response
                .data
                .into_iter()
                .map(|symbol| match symbol.strip_suffix(USDT) {
                    Some(base) if !base.is_empty() => ListedMarket::new(base, USDT, true),
                    // Non-USDT symbol: keep it for the total-markets count, but
                    // the base/quote cannot be split, so mark the quote unknown.
                    _ => ListedMarket::new(symbol, "", true),
                })
        })
    }
}

/// Poloniex
#[allow(clippy::type_complexity)]
type PoloniexResponse = Vec<(
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    u64,
    u64,
    String,
    String,
    u64,
    u64,
)>;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PoloniexMarket {
    base_currency_name: String,
    quote_currency_name: String,
    state: String,
}
type PoloniexListing = Vec<PoloniexMarket>;

impl IsExchange for Poloniex {
    fn get_base_url(&self) -> &str {
        "https://api.poloniex.com/markets/BASE_ASSET_QUOTE_ASSET/candles?interval=MINUTE_1&startTime=START_TIME&endTime=END_TIME"
    }

    fn format_start_time(&self, timestamp: u64) -> String {
        // Convert seconds to milliseconds.
        timestamp.saturating_mul(1000).to_string()
    }

    fn format_end_time(&self, timestamp: u64) -> String {
        // Convert seconds to milliseconds and add 1 millisecond.
        timestamp.saturating_mul(1000).saturating_add(1).to_string()
    }

    fn extract_rate(&self, bytes: &[u8]) -> Result<u64, ExtractError> {
        extract_rate(bytes, |response: PoloniexResponse| {
            response
                .first()
                .map(|kline| ExtractedValue::Str(kline.2.clone()))
        })
    }

    fn listing_url(&self) -> &str {
        "https://api.poloniex.com/markets"
    }

    fn extract_listed_usdt_bases(&self, bytes: &[u8]) -> Result<ListedPairs, ExtractError> {
        extract_listed_pairs(bytes, |markets: PoloniexListing| {
            markets.into_iter().map(|market| {
                ListedMarket::new(
                    market.base_currency_name,
                    market.quote_currency_name,
                    market.state == "NORMAL",
                )
            })
        })
    }

    // Drop USDS-USDT. Poloniex's USDS-USDT market is dead — it has no recent
    // trades and its ticker/candles return a stale, off-peg price (~0.97).
    // Because a (stale) candle is still returned, the fetch *succeeds*, so this
    // silently feeds an ~3%-off sample into the stablecoin rate rather than
    // erroring. USDS-USDT is covered by other exchanges; keep only USDT-USDC.
    fn supported_stablecoin_pairs(&self) -> &[(&str, &str)] {
        &[(USDT, USDC)]
    }
}

/// Crypto
#[derive(Deserialize)]
struct CryptoResponse {
    result: CryptoResponseResult,
}

#[derive(Deserialize)]
struct CryptoResponseResult {
    data: Vec<CryptoResponseResultData>,
}

#[derive(Deserialize)]
struct CryptoResponseResultData {
    o: String,
}

#[derive(Deserialize)]
struct CryptoComInstrument {
    base_ccy: String,
    quote_ccy: String,
    inst_type: String,
    #[serde(default)]
    tradable: bool,
}
#[derive(Deserialize)]
struct CryptoComInstrumentsResult {
    data: Vec<CryptoComInstrument>,
}
#[derive(Deserialize)]
struct CryptoComInstrumentsResponse {
    result: CryptoComInstrumentsResult,
}

impl IsExchange for CryptoCom {
    fn get_base_url(&self) -> &str {
        "https://api.crypto.com/exchange/v1/public/get-candlestick?instrument_name=BASE_ASSET_QUOTE_ASSET&timeframe=1m&start_ts=START_TIME&count=1"
    }

    fn supports_ipv6(&self) -> bool {
        true
    }

    fn format_start_time(&self, timestamp: u64) -> String {
        // Convert seconds to milliseconds.
        timestamp.saturating_mul(1000).to_string()
    }

    fn extract_rate(&self, bytes: &[u8]) -> Result<u64, ExtractError> {
        extract_rate(bytes, |response: CryptoResponse| {
            response
                .result
                .data
                .first()
                .map(|kline| ExtractedValue::Str(kline.o.clone()))
        })
    }

    fn listing_url(&self) -> &str {
        "https://api.crypto.com/exchange/v1/public/get-instruments"
    }

    fn extract_listed_usdt_bases(&self, bytes: &[u8]) -> Result<ListedPairs, ExtractError> {
        extract_listed_pairs(bytes, |response: CryptoComInstrumentsResponse| {
            response
                .result
                .data
                .into_iter()
                // Keep only spot currency pairs; the same endpoint also returns
                // ~300 derivatives (PERPETUAL_SWAP, FUTURE) that are not spot
                // markets and must not count toward total_markets.
                .filter(|instrument| instrument.inst_type == "CCY_PAIR")
                .map(|instrument| {
                    ListedMarket::new(
                        instrument.base_ccy,
                        instrument.quote_ccy,
                        instrument.tradable,
                    )
                })
        })
    }

    // Crypto.com's only stablecoin pair, USDT-USDC, is dead: no trades at all in
    // sampled 30-day windows, yet it still returns a forward-filled candle, so
    // the fetch *succeeds* with a frozen price fed into the USDC median. Unlike
    // the other thin USDT-USDC sources (kept, and made honest by empty-window
    // and freshness handling), this market is not expected to recover, so the
    // source is dropped outright. USDC is covered by the liquid USDC-USDT
    // markets on the other exchanges.
    fn supported_stablecoin_pairs(&self) -> &[(&str, &str)] {
        &[]
    }
}

/// Bitget
#[allow(clippy::type_complexity)]
type BitgetResponseDataEntry = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
);

#[derive(Deserialize)]
struct BitgetResponse {
    data: Vec<BitgetResponseDataEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BitgetSymbol {
    base_coin: String,
    quote_coin: String,
    status: String,
}
#[derive(Deserialize)]
struct BitgetSymbolsResponse {
    data: Vec<BitgetSymbol>,
}

impl IsExchange for Bitget {
    fn get_base_url(&self) -> &str {
        "https://api.bitget.com/api/v2/spot/market/history-candles?symbol=BASE_ASSETQUOTE_ASSET&granularity=1min&endTime=END_TIME&limit=1"
    }

    fn format_end_time(&self, timestamp: u64) -> String {
        // Convert seconds to milliseconds and add one minute.
        timestamp
            .saturating_mul(1000)
            .saturating_add(60_000)
            .to_string()
    }

    fn extract_rate(&self, bytes: &[u8]) -> Result<u64, ExtractError> {
        extract_rate(bytes, |response: BitgetResponse| {
            response
                .data
                .first()
                .map(|kline| ExtractedValue::Str(kline.1.clone()))
        })
    }

    fn listing_url(&self) -> &str {
        "https://api.bitget.com/api/v2/spot/public/symbols"
    }

    fn extract_listed_usdt_bases(&self, bytes: &[u8]) -> Result<ListedPairs, ExtractError> {
        extract_listed_pairs(bytes, |response: BitgetSymbolsResponse| {
            response
                .data
                .into_iter()
                .map(|symbol| {
                    ListedMarket::new(
                        symbol.base_coin,
                        symbol.quote_coin,
                        symbol.status == "online",
                    )
                })
        })
    }

    fn supports_ipv6(&self) -> bool {
        true
    }
}

/// Digifinex
#[derive(Deserialize)]
struct DigifinexResponse {
    data: Vec<(u64, f64, f64, f64, f64, f64)>,
}

#[derive(Deserialize)]
struct DigifinexSymbol {
    base_asset: String,
    quote_asset: String,
    status: String,
}
#[derive(Deserialize)]
struct DigifinexSymbolsResponse {
    symbol_list: Vec<DigifinexSymbol>,
}

impl IsExchange for Digifinex {
    fn get_base_url(&self) -> &str {
        "https://openapi.digifinex.com/v3/kline?symbol=BASE_ASSET_QUOTE_ASSET&period=1&start_time=START_TIME&end_time=END_TIME"
    }

    fn extract_rate(&self, bytes: &[u8]) -> Result<u64, ExtractError> {
        extract_rate(bytes, |response: DigifinexResponse| {
            response
                .data
                .first()
                .map(|kline| ExtractedValue::Float(kline.5))
        })
    }

    fn listing_url(&self) -> &str {
        "https://openapi.digifinex.com/v3/spot/symbols"
    }

    fn extract_listed_usdt_bases(&self, bytes: &[u8]) -> Result<ListedPairs, ExtractError> {
        extract_listed_pairs(bytes, |response: DigifinexSymbolsResponse| {
            response
                .symbol_list
                .into_iter()
                .map(|symbol| {
                    ListedMarket::new(
                        symbol.base_asset,
                        symbol.quote_asset,
                        symbol.status == "TRADING",
                    )
                })
        })
    }

    fn supports_ipv6(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod test {
    use crate::utils::test::load_file;

    use super::*;

    /// The function test if the macro correctly generates the
    /// [core::fmt::Display] trait's implementation for [Exchange].
    #[test]
    fn exchange_to_string_returns_name() {
        let exchange = Exchange::Coinbase(Coinbase);
        assert_eq!(exchange.to_string(), "Coinbase");
        let exchange = Exchange::KuCoin(KuCoin);
        assert_eq!(exchange.to_string(), "KuCoin");
        let exchange = Exchange::Okx(Okx);
        assert_eq!(exchange.to_string(), "Okx");
        let exchange = Exchange::GateIo(GateIo);
        assert_eq!(exchange.to_string(), "GateIo");
        let exchange = Exchange::Mexc(Mexc);
        assert_eq!(exchange.to_string(), "Mexc");
        let exchange = Exchange::Poloniex(Poloniex);
        assert_eq!(exchange.to_string(), "Poloniex");
        let exchange = Exchange::CryptoCom(CryptoCom);
        assert_eq!(exchange.to_string(), "CryptoCom");
        let exchange = Exchange::Bitget(Bitget);
        assert_eq!(exchange.to_string(), "Bitget");
        let exchange = Exchange::Digifinex(Digifinex);
        assert_eq!(exchange.to_string(), "Digifinex");
    }

    /// The function tests if the if the macro correctly generates derive copies by
    /// verifying that the exchanges return the correct query string.
    #[test]
    fn query_string() {
        // Note that the seconds are ignored, setting the considered timestamp to 1661523960.
        let timestamp = 1661524016;

        let coinbase = Coinbase;
        let query_string = coinbase.get_url("btc", "icp", timestamp);
        // Window of already-closed minutes ending one minute before the
        // (floored) requested timestamp: start = 1661523960 - 360, end =
        // 1661523960 - 60.
        assert_eq!(query_string, "https://api.exchange.coinbase.com/products/BTC-ICP/candles?granularity=60&start=1661523600&end=1661523900");

        let kucoin = KuCoin;
        let query_string = kucoin.get_url("btc", "icp", timestamp);
        assert_eq!(query_string, "https://api.kucoin.com/api/v1/market/candles?symbol=BTC-ICP&type=1min&startAt=1661523600&endAt=1661523901");

        let okx = Okx;
        let query_string = okx.get_url("btc", "icp", timestamp);
        assert_eq!(query_string, "https://www.okx.com/api/v5/market/history-candles?instId=BTC-ICP&bar=1m&before=1661523899999&after=1661523960001");

        let gate_io = GateIo;
        let query_string = gate_io.get_url("btc", "icp", timestamp);
        assert_eq!(query_string, "https://api.gateio.ws/api/v4/spot/candlesticks?currency_pair=BTC_ICP&interval=1m&from=1661523960&to=1661523960");

        let mexc = Mexc;
        let query_string = mexc.get_url("btc", "icp", timestamp);
        assert_eq!(query_string, "https://api.mexc.com/api/v3/klines?symbol=BTCICP&interval=1m&startTime=1661523960&limit=1");

        let poloniex = Poloniex;
        let query_string = poloniex.get_url("btc", "icp", timestamp);
        assert_eq!(query_string, "https://api.poloniex.com/markets/BTC_ICP/candles?interval=MINUTE_1&startTime=1661523960000&endTime=1661523960001");

        let crypto = CryptoCom;
        let query_string = crypto.get_url("btc", "icp", timestamp);
        assert_eq!(query_string, "https://api.crypto.com/exchange/v1/public/get-candlestick?instrument_name=BTC_ICP&timeframe=1m&start_ts=1661523960000&count=1");

        let bitget = Bitget;
        let query_string = bitget.get_url("icp", "usdt", timestamp);
        assert_eq!(query_string, "https://api.bitget.com/api/v2/spot/market/history-candles?symbol=ICPUSDT&granularity=1min&endTime=1661524020000&limit=1");

        let digifinex = Digifinex;
        let query_string = digifinex.get_url("icp", "usdt", timestamp);
        assert_eq!(query_string, "https://openapi.digifinex.com/v3/kline?symbol=ICP_USDT&period=1&start_time=1661523960&end_time=1661523960");
    }

    /// The function test if the information about IPv6 support is correct.
    #[test]
    fn ipv6_support() {
        let coinbase = Coinbase;
        assert!(coinbase.supports_ipv6());
        let kucoin = KuCoin;
        assert!(kucoin.supports_ipv6());
        let okx = Okx;
        assert!(okx.supports_ipv6());
        let gate_io = GateIo;
        assert!(!gate_io.supports_ipv6());
        let mexc = Mexc;
        assert!(!mexc.supports_ipv6());
        let poloniex = Poloniex;
        assert!(!poloniex.supports_ipv6());
        let crypto = CryptoCom;
        assert!(crypto.supports_ipv6());
        let bitget = Bitget;
        assert!(bitget.supports_ipv6());
        let digifinex = Digifinex;
        assert!(digifinex.supports_ipv6());
    }

    /// The function tests if the USD asset type is correct.
    #[test]
    fn supported_usd_asset_type() {
        let coinbase = Coinbase;
        assert_eq!(coinbase.supported_usd_asset(), usd_asset());
        let kucoin = KuCoin;
        assert_eq!(kucoin.supported_usd_asset(), usdt_asset());
        let okx = Okx;
        assert_eq!(okx.supported_usd_asset(), usdt_asset());
        let gate_io = GateIo;
        assert_eq!(gate_io.supported_usd_asset(), usdt_asset());
        let mexc = Mexc;
        assert_eq!(mexc.supported_usd_asset(), usdt_asset());
        let poloniex = Poloniex;
        assert_eq!(poloniex.supported_usd_asset(), usdt_asset());
        let crypto = CryptoCom;
        assert_eq!(crypto.supported_usd_asset(), usdt_asset());
        let bitget = Bitget;
        assert_eq!(bitget.supported_usd_asset(), usdt_asset());
        let digifinex = Digifinex;
        assert_eq!(digifinex.supported_usd_asset(), usdt_asset());
    }

    /// The function tests if the supported stablecoins are correct.
    #[test]
    fn supported_stablecoin_pairs() {
        let coinbase = Coinbase;
        assert_eq!(coinbase.supported_stablecoin_pairs(), &[(USDT, USDC)]);
        let kucoin = KuCoin;
        assert_eq!(kucoin.supported_stablecoin_pairs(), &[(USDC, USDT)]);
        let okx = Okx;
        assert_eq!(
            okx.supported_stablecoin_pairs(),
            &[(USDS, USDT), (USDC, USDT)]
        );
        let gate_io = GateIo;
        assert_eq!(
            gate_io.supported_stablecoin_pairs(),
            &[(USDS, USDT), (USDC, USDT)]
        );
        let mexc = Mexc;
        assert_eq!(
            mexc.supported_stablecoin_pairs(),
            &[(USDS, USDT), (USDC, USDT)]
        );
        let poloniex = Poloniex;
        assert_eq!(poloniex.supported_stablecoin_pairs(), &[(USDT, USDC)]);
        let crypto = CryptoCom;
        // Crypto.com's USDT-USDC market is dead (see impl); dropped outright.
        assert_eq!(crypto.supported_stablecoin_pairs(), &[]);
        let bitget = Bitget;
        assert_eq!(
            bitget.supported_stablecoin_pairs(),
            &[(USDS, USDT), (USDC, USDT)]
        );
        let digifinex = Digifinex;
        assert_eq!(
            digifinex.supported_stablecoin_pairs(),
            &[(USDS, USDT), (USDC, USDT)]
        );
    }

    /// The function tests if the Coinbase struct returns the correct exchange rate.
    #[test]
    fn extract_rate_from_coinbase() {
        let coinbase = Coinbase;
        // The fixture holds several candles newest-first (as Coinbase returns
        // them); the rate must come from the most recent one, i.e. the open of
        // the first entry (49.18), not an older candle.
        let query_response = load_file("test-data/exchanges/coinbase.json");
        let extracted_rate = coinbase.extract_rate(&query_response);
        assert!(matches!(extracted_rate, Ok(rate) if rate == 49_180_000_000));
    }

    /// The function tests if the KuCoin struct returns the correct exchange rate.
    #[test]
    fn extract_rate_from_kucoin() {
        let kucoin = KuCoin;
        let query_response = load_file("test-data/exchanges/kucoin.json");
        let extracted_rate = kucoin.extract_rate(&query_response);
        assert!(matches!(extracted_rate, Ok(rate) if rate == 345_426_000_000));
    }

    /// KuCoin returns candles newest-first, so when the closed-minute window
    /// yields several candles `extract_rate` must pick the first (most recent).
    #[test]
    fn extract_rate_from_kucoin_picks_newest_candle() {
        let kucoin = KuCoin;
        let response = br#"{"code":"200000","data":[
            ["1620296820","345.426","344.396","345.426","344.096","280.0","96000.0"],
            ["1620296760","340.000","339.000","341.000","338.000","100.0","34000.0"]
        ]}"#;
        let extracted_rate = kucoin.extract_rate(response);
        assert!(matches!(extracted_rate, Ok(rate) if rate == 345_426_000_000));
    }

    /// The function tests if the OKX struct returns the correct exchange rate.
    #[test]
    fn extract_rate_from_okx() {
        let okx = Okx;
        let query_response = load_file("test-data/exchanges/okx.json");
        let extracted_rate = okx.extract_rate(&query_response);
        assert!(matches!(extracted_rate, Ok(rate) if rate == 41_960_000_000));
    }

    /// The function tests if the GateIo struct returns the correct exchange rate.
    #[test]
    fn extract_rate_from_gate_io() {
        let gate_io = GateIo;
        let query_response = load_file("test-data/exchanges/gateio.json");
        let extracted_rate = gate_io.extract_rate(&query_response);
        assert!(matches!(extracted_rate, Ok(rate) if rate == 42_640_000_000));
    }

    /// The function tests if the Mexc struct returns the correct exchange rate.
    #[test]
    fn extract_rate_from_mexc() {
        let mexc = Mexc;
        let query_response = load_file("test-data/exchanges/mexc.json");
        let extracted_rate = mexc.extract_rate(&query_response);
        assert!(matches!(extracted_rate, Ok(rate) if rate == 46_101_000_000));
    }

    /// The function tests if the Poloniex struct returns the correct exchange rate.
    #[test]
    fn extract_rate_from_poloniex() {
        let poloniex = Poloniex;
        let query_response = load_file("test-data/exchanges/poloniex.json");
        let extracted_rate = poloniex.extract_rate(&query_response);
        assert!(matches!(extracted_rate, Ok(rate) if rate == 46_022_000_000));
    }

    /// The function tests if the Crypto struct returns the correct exchange rate.
    #[test]
    fn extract_rate_from_crypto() {
        let crypto = CryptoCom;
        let query_response = load_file("test-data/exchanges/crypto.json");
        let extracted_rate = crypto.extract_rate(&query_response);
        assert!(matches!(extracted_rate, Ok(rate) if rate == 47_328_300_000));
    }

    /// The function tests if the Bitget struct returns the correct exchange rate.
    #[test]
    fn extract_rate_from_bitget() {
        let bitget = Bitget;
        let query_response = load_file("test-data/exchanges/bitget.json");
        let extracted_rate = bitget.extract_rate(&query_response);
        assert!(matches!(extracted_rate, Ok(rate) if rate == 13_123_000_000));
    }

    /// The function tests if the Digifinex struct returns the correct exchange rate.
    #[test]
    fn extract_rate_from_digifinex() {
        let digifinex = Digifinex;
        let query_response = load_file("test-data/exchanges/digifinex.json");
        let extracted_rate = digifinex.extract_rate(&query_response);
        assert!(matches!(extracted_rate, Ok(rate) if rate == 11_357_000_000));
    }

    /// Asserts the common shape of every listing fixture: each holds four spot
    /// markets — two tradable USDT pairs (BTC, ETH), one tradable non-USDT pair,
    /// and one non-tradable USDT pair — so a correct parser yields exactly
    /// `{BTC, ETH}` as the gating set while still counting all four (or, for
    /// Crypto.com, four spot pairs plus a derivative that must be excluded) in
    /// `total_markets`.
    fn assert_lists_btc_and_eth(exchange: &impl IsExchange, file: &str) {
        let body = load_file(file);
        let listed = exchange
            .extract_listed_usdt_bases(&body)
            .expect("should parse the listing fixture");
        let expected: BTreeSet<String> = ["BTC", "ETH"].iter().map(|s| s.to_string()).collect();
        assert_eq!(listed.bases, expected, "unexpected USDT bases for {file}");
        assert_eq!(
            listed.total_markets, 4,
            "unexpected total markets for {file}"
        );
    }

    /// Pins each exchange's listing endpoint URL.
    #[test]
    fn listing_urls() {
        assert_eq!(
            Coinbase.listing_url(),
            "https://api.exchange.coinbase.com/products"
        );
        assert_eq!(
            KuCoin.listing_url(),
            "https://api.kucoin.com/api/v1/symbols"
        );
        assert_eq!(
            Okx.listing_url(),
            "https://www.okx.com/api/v5/public/instruments?instType=SPOT"
        );
        assert_eq!(
            GateIo.listing_url(),
            "https://api.gateio.ws/api/v4/spot/currency_pairs"
        );
        assert_eq!(
            Mexc.listing_url(),
            "https://api.mexc.com/api/v3/defaultSymbols"
        );
        assert_eq!(Poloniex.listing_url(), "https://api.poloniex.com/markets");
        assert_eq!(
            CryptoCom.listing_url(),
            "https://api.crypto.com/exchange/v1/public/get-instruments"
        );
        assert_eq!(
            Bitget.listing_url(),
            "https://api.bitget.com/api/v2/spot/public/symbols"
        );
        assert_eq!(
            Digifinex.listing_url(),
            "https://openapi.digifinex.com/v3/spot/symbols"
        );
    }

    #[test]
    fn extract_listed_usdt_bases_from_coinbase() {
        assert_lists_btc_and_eth(&Coinbase, "test-data/exchanges/listings/coinbase.json");
    }

    #[test]
    fn extract_listed_usdt_bases_from_kucoin() {
        assert_lists_btc_and_eth(&KuCoin, "test-data/exchanges/listings/kucoin.json");
    }

    #[test]
    fn extract_listed_usdt_bases_from_okx() {
        assert_lists_btc_and_eth(&Okx, "test-data/exchanges/listings/okx.json");
    }

    #[test]
    fn extract_listed_usdt_bases_from_gate_io() {
        assert_lists_btc_and_eth(&GateIo, "test-data/exchanges/listings/gateio.json");
    }

    #[test]
    fn extract_listed_usdt_bases_from_mexc() {
        assert_lists_btc_and_eth(&Mexc, "test-data/exchanges/listings/mexc.json");
    }

    #[test]
    fn extract_listed_usdt_bases_from_poloniex() {
        assert_lists_btc_and_eth(&Poloniex, "test-data/exchanges/listings/poloniex.json");
    }

    /// Crypto.com's fixture additionally includes a `PERPETUAL_SWAP` entry; the
    /// `total_markets == 4` assertion confirms derivatives are excluded.
    #[test]
    fn extract_listed_usdt_bases_from_crypto() {
        assert_lists_btc_and_eth(&CryptoCom, "test-data/exchanges/listings/cryptocom.json");
    }

    #[test]
    fn extract_listed_usdt_bases_from_bitget() {
        assert_lists_btc_and_eth(&Bitget, "test-data/exchanges/listings/bitget.json");
    }

    #[test]
    fn extract_listed_usdt_bases_from_digifinex() {
        assert_lists_btc_and_eth(&Digifinex, "test-data/exchanges/listings/digifinex.json");
    }

    /// The function tests the ability of an [Exchange] to encode the context to be sent
    /// to the exchange transform function.
    #[test]
    fn encode_context() {
        let exchange = Exchange::Coinbase(Coinbase);
        let bytes = exchange
            .encode_context()
            .expect("should encode Coinbase's index in EXCHANGES");
        let hex_string = hex::encode(bytes);
        assert_eq!(hex_string, "4449444c0001780000000000000000");
    }

    /// The function tests the ability of [Exchange] to encode a response body from the
    /// exchange transform function.
    #[test]
    fn encode_response() {
        let bytes = Exchange::encode_response(100).expect("should be able to encode value");
        let hex_string = hex::encode(bytes);
        assert_eq!(hex_string, "4449444c0001786400000000000000");
    }

    /// The function tests the ability of [Exchange] to decode a context in the exchange
    /// transform function.
    #[test]
    fn decode_context() {
        let hex_string = "4449444c0001780100000000000000";
        let bytes = hex::decode(hex_string).expect("should be able to decode");
        let result = Exchange::decode_context(&bytes);
        assert!(matches!(result, Ok(index) if index == 1));
    }

    /// The function tests the ability of [Exchange] to decode a response body from the
    /// exchange transform function.
    #[test]
    fn decode_response() {
        let hex_string = "4449444c0001786400000000000000";
        let bytes = hex::decode(hex_string).expect("should be able to decode");
        let result = Exchange::decode_response(&bytes);
        assert!(matches!(result, Ok(rate) if rate == 100));
    }

    #[test]
    fn max_response_bytes() {
        let exchange = Exchange::Coinbase(Coinbase);
        assert_eq!(exchange.max_response_bytes(), 3 * ONE_KIB);
        let exchange = Exchange::KuCoin(KuCoin);
        assert_eq!(exchange.max_response_bytes(), 3 * ONE_KIB);
        let exchange = Exchange::Okx(Okx);
        assert_eq!(exchange.max_response_bytes(), 3 * ONE_KIB);
        let exchange = Exchange::GateIo(GateIo);
        assert_eq!(exchange.max_response_bytes(), 3 * ONE_KIB);
        let exchange = Exchange::Mexc(Mexc);
        assert_eq!(exchange.max_response_bytes(), 3 * ONE_KIB);
        let exchange = Exchange::Poloniex(Poloniex);
        assert_eq!(exchange.max_response_bytes(), 3 * ONE_KIB);
        let exchange = Exchange::CryptoCom(CryptoCom);
        assert_eq!(exchange.max_response_bytes(), 3 * ONE_KIB);
        let exhange = Exchange::Bitget(Bitget);
        assert!(exhange.max_response_bytes() <= 3 * ONE_KIB);
        let exhange = Exchange::Digifinex(Digifinex);
        assert!(exhange.max_response_bytes() <= 3 * ONE_KIB);
    }

    #[test]
    #[cfg(not(feature = "ipv4-support"))]
    fn is_available() {
        let available_exchanges_count = EXCHANGES.iter().filter(|e| e.is_available()).count();
        assert_eq!(available_exchanges_count, 6);
    }

    #[test]
    #[cfg(feature = "ipv4-support")]
    fn is_available_ipv4() {
        let available_exchanges_count = EXCHANGES.iter().filter(|e| e.is_available()).count();
        assert_eq!(available_exchanges_count, 9);
    }
}
