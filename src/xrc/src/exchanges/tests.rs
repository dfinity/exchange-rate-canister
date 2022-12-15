use crate::utils::test::load_file;

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
}

/// The function tests if the Binance struct returns the correct exchange rate.
#[test]
fn extract_rate_from_binance() {
    let binance = Binance;
    let query_response = load_file("test-data/exchanges/binance.json");
    let extracted_rate = binance.extract_rate(&query_response);
    assert!(matches!(extracted_rate, Ok(rate) if rate == 41_960_000_000));
}

/// The function tests if the Coinbase struct returns the correct exchange rate.
#[test]
fn extract_rate_from_coinbase() {
    let coinbase = Coinbase;
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

/// The function tests the ability of an [Exchange] to encode the context to be sent
/// to the exchange transform function.
#[test]
fn encode_context() {
    let exchange = Exchange::Coinbase(Coinbase);
    let bytes = exchange
        .encode_context()
        .expect("should encode Coinbase's index in EXCHANGES");
    let hex_string = hex::encode(bytes);
    assert_eq!(hex_string, "4449444c0001780100000000000000");
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
    let exchange = Exchange::Binance(Binance);
    assert_eq!(exchange.max_response_bytes(), ONE_KIB);
    let exchange = Exchange::Coinbase(Coinbase);
    assert_eq!(exchange.max_response_bytes(), ONE_KIB);
    let exchange = Exchange::KuCoin(KuCoin);
    assert_eq!(exchange.max_response_bytes(), 2 * ONE_KIB);
    let exchange = Exchange::Okx(Okx);
    assert_eq!(exchange.max_response_bytes(), ONE_KIB);
    let exchange = Exchange::GateIo(GateIo);
    assert_eq!(exchange.max_response_bytes(), ONE_KIB);
    let exchange = Exchange::Mexc(Mexc);
    assert_eq!(exchange.max_response_bytes(), ONE_KIB);
}
