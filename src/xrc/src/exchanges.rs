use ic_cdk::export::candid::{decode_args, encode_args, Error as CandidError};
use jaq_core::Val;

use crate::candid::{Asset, AssetClass};
use crate::{jq, DAI, USDC, USDT};
use crate::{ExtractError, RATE_UNIT};

/// This macro generates the necessary boilerplate when adding an exchange to this module.

macro_rules! exchanges {
    ($($name:ident),*) => {
        /// Enum that contains all of the supported cryptocurrency exchanges.
        #[derive(PartialEq)]
        pub enum Exchange {
            $(
                #[allow(missing_docs)]
                $name($name),
            )*
        }

        $(
            #[derive(PartialEq)]
            pub struct $name;
        )*

        impl core::fmt::Display for Exchange {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $(Exchange::$name(_) => write!(f, stringify!($name))),*,
                }
            }
        }

        /// Contains all of the known exchanges that can be found in the
        /// [Exchange] enum.
        pub const EXCHANGES: &'static [Exchange] = &[
            $(Exchange::$name($name)),*
        ];


        /// Implements the core functionality of the generated `Exchange` enum.
        impl Exchange {

            /// Retrieves the position of the exchange in the EXCHANGES array.
            pub fn get_index(&self) -> usize {
                EXCHANGES.iter().position(|e| e == self).expect("should contain the exchange")
            }

            /// This method routes the request to the correct exchange's [IsExchange::get_url] method.
            pub fn get_url(&self, base_asset: &str, quote_asset: &str, timestamp: u64) -> String {
                match self {
                    $(Exchange::$name(exchange) => exchange.get_url(base_asset, quote_asset, timestamp)),*,
                }
            }

            /// This method routes the response's body and the timestamp to the correct exchange's
            /// [IsExchange::extract_rate].
            pub fn extract_rate(&self, bytes: &[u8]) -> Result<u64, ExtractError> {
                match self {
                    $(Exchange::$name(exchange) => exchange.extract_rate(bytes)),*,
                }
            }

            /// This method invokes the exchange's [IsExchange::supports_ipv6] function.
            pub fn supports_ipv6(&self) -> bool {
                match self {
                    $(Exchange::$name(exchange) => exchange.supports_ipv6()),*,
                }
            }

            /// This method invokes the exchange's [IsExchange::supported_usd_asset_type] function.
            pub fn supported_usd_asset_type(&self) -> Asset {
                match self {
                    $(Exchange::$name(exchange) => exchange.supported_usd_asset()),*,
                }
            }

            /// This method invoices the exchange's [IsExchange::supported_stablecoin_pairs] function.
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
        }
    }

}

exchanges! { Binance, Coinbase, KuCoin, Okx, GateIo, Mexc, Bybit }

/// The base URL may contain the following placeholders:
/// `BASE_ASSET`: This string must be replaced with the base asset string in the request.
const BASE_ASSET: &str = "BASE_ASSET";
/// `QUOTE_ASSET`: This string must be replaced with the quote asset string in the request.
const QUOTE_ASSET: &str = "QUOTE_ASSET";
/// `START_TIME`: This string must be replaced with the start time derived from the timestamp in the request.
const START_TIME: &str = "START_TIME";
/// `END_TIME`: This string must be replaced with the end time derived from the timestamp in the request.
const END_TIME: &str = "END_TIME";

/// This trait is use to provide the basic methods needed for an exchange.
trait IsExchange {
    /// The filter template that is provided to [IsExchange::extract_rate]. Default implemenation
    /// expects the open price at position 1 in the single data record.
    fn get_filter(&self) -> &str {
        ".data[0][1] | tonumber"
    }

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

    /// A default implementation to extract the rate from the response's body
    /// using the base filter and [jq::extract].
    fn extract_rate(&self, bytes: &[u8]) -> Result<u64, ExtractError> {
        let filter = self.get_filter();
        let value = jq::extract(bytes, filter)?;
        match value {
            Val::Num(rc) => match (*rc).as_f64() {
                Some(rate) => Ok((rate * RATE_UNIT as f64) as u64),
                None => Err(ExtractError::InvalidNumericRate {
                    filter: filter.to_string(),
                    value: rc.to_string(),
                }),
            },
            _ => Err(ExtractError::RateNotFound {
                filter: filter.to_string(),
            }),
        }
    }

    /// Indicates if the exchange supports IPv6.
    fn supports_ipv6(&self) -> bool {
        false
    }

    /// Return the exchange's supported USD asset type.
    fn supported_usd_asset(&self) -> Asset {
        Asset {
            symbol: USDT.to_string(),
            class: AssetClass::Cryptocurrency,
        }
    }

    fn supported_stablecoin_pairs(&self) -> &[(&str, &str)] {
        &[(DAI, USDT), (USDC, USDT)]
    }
}

/// Binance
impl IsExchange for Binance {
    fn get_filter(&self) -> &str {
        ".[0][1] | tonumber"
    }

    fn get_base_url(&self) -> &str {
        "https://api.binance.com/api/v3/klines?symbol=BASE_ASSETQUOTE_ASSET&interval=1m&startTime=START_TIME&endTime=END_TIME"
    }

    fn format_start_time(&self, timestamp: u64) -> String {
        // Convert seconds to milliseconds.
        timestamp.saturating_mul(1000).to_string()
    }

    fn format_end_time(&self, timestamp: u64) -> String {
        // Convert seconds to milliseconds.
        timestamp.saturating_mul(1000).to_string()
    }
}

/// Coinbase
impl IsExchange for Coinbase {
    fn get_filter(&self) -> &str {
        ".[0][3]"
    }

    fn get_base_url(&self) -> &str {
        "https://api.pro.coinbase.com/products/BASE_ASSET-QUOTE_ASSET/candles?granularity=60&start=START_TIME&end=END_TIME"
    }

    fn supports_ipv6(&self) -> bool {
        true
    }

    fn supported_usd_asset(&self) -> Asset {
        Asset {
            symbol: "USD".to_string(),
            class: AssetClass::FiatCurrency,
        }
    }

    fn supported_stablecoin_pairs(&self) -> &[(&str, &str)] {
        &[(USDT, USDC)]
    }
}

/// KuCoin
impl IsExchange for KuCoin {
    fn get_base_url(&self) -> &str {
        "https://api.kucoin.com/api/v1/market/candles?symbol=BASE_ASSET-QUOTE_ASSET&type=1min&startAt=START_TIME&endAt=END_TIME"
    }

    fn format_end_time(&self, timestamp: u64) -> String {
        // In order to include the end time, a second must be added.
        timestamp.saturating_add(1).to_string()
    }

    fn supports_ipv6(&self) -> bool {
        true
    }

    fn supported_stablecoin_pairs(&self) -> &[(&str, &str)] {
        &[(USDC, USDT), (USDT, DAI)]
    }
}

/// OKX
impl IsExchange for Okx {
    fn get_base_url(&self) -> &str {
        // Counterintuitively, "after" specifies the end time, and "before" specifies the start time.
        "https://www.okx.com/api/v5/market/history-candles?instId=BASE_ASSET-QUOTE_ASSET&bar=1m&before=START_TIME&after=END_TIME"
    }

    fn format_start_time(&self, timestamp: u64) -> String {
        // Convert seconds to milliseconds and subtract 1 millisecond.
        timestamp.saturating_mul(1000).saturating_sub(1).to_string()
    }

    fn format_end_time(&self, timestamp: u64) -> String {
        // Convert seconds to milliseconds and add 1 millisecond.
        timestamp.saturating_mul(1000).saturating_add(1).to_string()
    }

    fn supports_ipv6(&self) -> bool {
        true
    }
}

/// Gate.io
impl IsExchange for GateIo {
    fn get_filter(&self) -> &str {
        ".[0][5] | tonumber"
    }

    fn get_base_url(&self) -> &str {
        "https://api.gateio.ws/api/v4/spot/candlesticks?currency_pair=BASE_ASSET_QUOTE_ASSET&interval=1m&from=START_TIME&to=END_TIME"
    }

    fn supported_stablecoin_pairs(&self) -> &[(&str, &str)] {
        &[(DAI, USDT)]
    }
}

/// MEXC
impl IsExchange for Mexc {
    fn get_base_url(&self) -> &str {
        "https://www.mexc.com/open/api/v2/market/kline?symbol=BASE_ASSET_QUOTE_ASSET&interval=1m&start_time=START_TIME&limit=1"
    }
}

/// Bybit
impl IsExchange for Bybit {
    fn get_filter(&self) -> &str {
        ".result.list[0][1] | tonumber"
    }

    fn get_base_url(&self) -> &str {
        "https://api.bybit.com/derivatives/v3/public/kline?category=linear&symbol=BASE_ASSETQUOTE_ASSET&interval=1&start=START_TIME&end=END_TIME"
    }

    fn format_start_time(&self, timestamp: u64) -> String {
        // Convert seconds to milliseconds.
        timestamp.saturating_mul(1000).to_string()
    }

    fn format_end_time(&self, timestamp: u64) -> String {
        // Convert seconds to milliseconds.
        timestamp.saturating_mul(1000).to_string()
    }

    fn supported_stablecoin_pairs(&self) -> &[(&str, &str)] {
        &[(USDC, USDT)]
    }
}

#[cfg(test)]
mod test {
    use super::*;

    /// The function test if the macro correctly generates the
    /// [core::fmt::Display] trait's implementation for [Exchange].
    #[test]
    fn exchange_to_string_returns_name() {
        let exchange = Exchange::Binance(Binance);
        assert_eq!(exchange.to_string(), "Binance");
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
        let exchange = Exchange::Bybit(Bybit);
        assert_eq!(exchange.to_string(), "Bybit");
    }

    /// The function tests if the if the macro correctly generates derive copies by
    /// verifying that the exchanges return the correct query string.
    #[test]
    fn query_string() {
        // Note that the seconds are ignored, setting the considered timestamp to 1661523960.
        let timestamp = 1661524016;
        let binance = Binance;
        let query_string = binance.get_url("btc", "icp", timestamp);
        assert_eq!(query_string, "https://api.binance.com/api/v3/klines?symbol=BTCICP&interval=1m&startTime=1661523960000&endTime=1661523960000");

        let coinbase = Coinbase;
        let query_string = coinbase.get_url("btc", "icp", timestamp);
        assert_eq!(query_string, "https://api.pro.coinbase.com/products/BTC-ICP/candles?granularity=60&start=1661523960&end=1661523960");

        let kucoin = KuCoin;
        let query_string = kucoin.get_url("btc", "icp", timestamp);
        assert_eq!(query_string, "https://api.kucoin.com/api/v1/market/candles?symbol=BTC-ICP&type=1min&startAt=1661523960&endAt=1661523961");

        let okx = Okx;
        let query_string = okx.get_url("btc", "icp", timestamp);
        assert_eq!(query_string, "https://www.okx.com/api/v5/market/history-candles?instId=BTC-ICP&bar=1m&before=1661523959999&after=1661523960001");

        let gate_io = GateIo;
        let query_string = gate_io.get_url("btc", "icp", timestamp);
        assert_eq!(query_string, "https://api.gateio.ws/api/v4/spot/candlesticks?currency_pair=BTC_ICP&interval=1m&from=1661523960&to=1661523960");

        let mexc = Mexc;
        let query_string = mexc.get_url("btc", "icp", timestamp);
        assert_eq!(query_string, "https://www.mexc.com/open/api/v2/market/kline?symbol=BTC_ICP&interval=1m&start_time=1661523960&limit=1");

        let bybit = Bybit;
        let query_string = bybit.get_url("btc", "icp", timestamp);
        assert_eq!(query_string, "https://api.bybit.com/derivatives/v3/public/kline?category=linear&symbol=BTCICP&interval=1&start=1661523960000&end=1661523960000");
    }

    /// The function test if the information about IPv6 support is correct.
    #[test]
    fn ipv6_support() {
        let binance = Binance;
        assert!(!binance.supports_ipv6());
        let coinbase = Coinbase;
        assert!(coinbase.supports_ipv6());
        let kucoin = KuCoin;
        assert!(kucoin.supports_ipv6());
        let okx = Okx;
        assert!(okx.supports_ipv6());
        let okx = Okx;
        assert!(okx.supports_ipv6());
        let gate_io = GateIo;
        assert!(!gate_io.supports_ipv6());
        let mexc = Mexc;
        assert!(!mexc.supports_ipv6());
        let bybit = Bybit;
        assert!(!bybit.supports_ipv6());
    }

    /// The function tests if the USD asset type is correct.
    #[test]
    fn supported_usd_asset_type() {
        let usdt_asset = Asset {
            symbol: USDT.to_string(),
            class: AssetClass::Cryptocurrency,
        };
        let binance = Binance;
        assert_eq!(binance.supported_usd_asset(), usdt_asset);
        let coinbase = Coinbase;
        assert_eq!(
            coinbase.supported_usd_asset(),
            Asset {
                symbol: "USD".to_string(),
                class: AssetClass::FiatCurrency
            }
        );
        let kucoin = KuCoin;
        assert_eq!(kucoin.supported_usd_asset(), usdt_asset);
        let okx = Okx;
        assert_eq!(okx.supported_usd_asset(), usdt_asset);
        let gate_io = GateIo;
        assert_eq!(gate_io.supported_usd_asset(), usdt_asset);
        let mexc = Mexc;
        assert_eq!(mexc.supported_usd_asset(), usdt_asset);
        let bybit = Bybit;
        assert_eq!(bybit.supported_usd_asset(), usdt_asset);
    }

    /// The function tests if the supported stablecoins are correct.
    #[test]
    fn supported_stablecoin_pairs() {
        let binance = Binance;
        assert_eq!(
            binance.supported_stablecoin_pairs(),
            &[(DAI, USDT), (USDC, USDT)]
        );
        let coinbase = Coinbase;
        assert_eq!(coinbase.supported_stablecoin_pairs(), &[(USDT, USDC)]);
        let kucoin = KuCoin;
        assert_eq!(
            kucoin.supported_stablecoin_pairs(),
            &[(USDC, USDT), (USDT, DAI)]
        );
        let okx = Okx;
        assert_eq!(
            okx.supported_stablecoin_pairs(),
            &[(DAI, USDT), (USDC, USDT)]
        );
        let gate_io = GateIo;
        assert_eq!(gate_io.supported_stablecoin_pairs(), &[(DAI, USDT)]);
        let mexc = Mexc;
        assert_eq!(
            mexc.supported_stablecoin_pairs(),
            &[(DAI, USDT), (USDC, USDT)]
        );
        let bybit = Bybit;
        assert_eq!(bybit.supported_stablecoin_pairs(), &[(USDC, USDT)]);
    }

    /// The function tests if the Binance struct returns the correct exchange rate.
    #[test]
    fn extract_rate_from_binance() {
        let binance = Binance;
        let query_response = r#"[[1637161920000,"41.96000000","42.07000000","41.96000000","42.06000000","771.33000000",1637161979999,"32396.87850000",63,"504.38000000","21177.00270000","0"]]"#
            .as_bytes();
        let extracted_rate = binance.extract_rate(query_response);
        assert!(matches!(extracted_rate, Ok(rate) if rate == 41_960_000_000));
    }

    /// The function tests if the Coinbase struct returns the correct exchange rate.
    #[test]
    fn extract_rate_from_coinbase() {
        let coinbase = Coinbase;
        let query_response = "[[1647734400,49.15,60.28,49.18,60.19,12.4941909]]".as_bytes();
        let extracted_rate = coinbase.extract_rate(query_response);
        assert!(matches!(extracted_rate, Ok(rate) if rate == 49_180_000_000));
    }

    /// The function tests if the Coinbase struct returns the correct exchange rate.
    #[test]
    fn extract_rate_from_kucoin() {
        let kucoin = KuCoin;
        let query_response = r#"{"code":"200000","data":[["1620296820","345.426","344.396","345.426", "344.096","280.47910557","96614.19641390067"]]}"#
            .as_bytes();
        let extracted_rate = kucoin.extract_rate(query_response);
        assert!(matches!(extracted_rate, Ok(rate) if rate == 345_426_000_000));
    }

    /// The function tests if the OKX struct returns the correct exchange rate.
    #[test]
    fn extract_rate_from_okx() {
        let okx = Okx;
        let query_response = r#"{"code":"0","msg":"","data":[["1637161920000","41.96","42.07","41.95","42.07","461.846542","19395.517323"]]}"#
            .as_bytes();
        let extracted_rate = okx.extract_rate(query_response);
        assert!(matches!(extracted_rate, Ok(rate) if rate == 41_960_000_000));
    }

    /// The function tests if the GateIo struct returns the correct exchange rate.
    #[test]
    fn extract_rate_from_gate_io() {
        let gate_io = GateIo;
        let query_response =
            r#"[["1620296820","4659.281408","42.61","42.64","42.55","42.64"]]"#.as_bytes();
        let extracted_rate = gate_io.extract_rate(query_response);
        assert!(matches!(extracted_rate, Ok(rate) if rate == 42_640_000_000));
    }

    /// The function tests if the Mexc struct returns the correct exchange rate.
    #[test]
    fn extract_rate_from_mexc() {
        let mexc = Mexc;
        let query_response = r#"{"code":200,"data":[[1620296820,"46.101","46.105","46.107","46.101","45.72","34.928"]]}"#.as_bytes();
        let extracted_rate = mexc.extract_rate(query_response);
        assert!(matches!(extracted_rate, Ok(rate) if rate == 46_101_000_000));
    }

    /// The function tests if the Bybit struct returns the correct exchange rate.
    #[test]
    fn extract_rate_from_bybit() {
        let bybit = Bybit;
        let query_response = r#"{"retCode":0,"retMsg":"OK","result":{"symbol":"ICPUSDT","category":"linear","list":[["1664890800000","46.13","46.14","46.13","46.14","114.2","701.188"]]},"retExtInfo":null,"time":1664894492539}"#.as_bytes();
        let extracted_rate = bybit.extract_rate(query_response);
        assert!(matches!(extracted_rate, Ok(rate) if rate == 46_130_000_000));
    }

    /// The function tests the ability of an [Exchange] to encode the context to be sent
    /// to the exchange transform function.
    #[test]
    fn encode_context() {
        let exchange = Exchange::Bybit(Bybit);
        let bytes = exchange
            .encode_context()
            .expect("should encode Bybit's index in EXCHANGES");
        let hex_string = hex::encode(bytes);
        assert_eq!(hex_string, "4449444c0001780600000000000000");
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
        let hex_string = "4449444c0001780600000000000000";
        let bytes = hex::decode(hex_string).expect("should be able to decode");
        let result = Exchange::decode_context(&bytes);
        assert!(matches!(result, Ok(index) if index == 6));
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
}
