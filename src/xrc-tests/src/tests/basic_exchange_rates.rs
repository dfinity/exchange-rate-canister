use ic_xrc_types::{Asset, AssetClass, GetExchangeRateRequest};

use crate::pocket::XrcTestEnv;
use crate::tests::{NUM_EXCHANGES, NUM_FOREX_SOURCES};
use crate::{mock_responses, ONE_DAY_SECONDS};

/// Setup:
/// * Deploy mock FOREX data providers and exchanges.
/// * Start the replica and deploy the XRC, configured to use the mock data sources
///
/// Runbook:
/// * Request exchange rate for various cryptocurrency and fiat currency pairs
/// * Assert that the returned rates correspond to the expected values
///
/// Success criteria:
/// * All queries return the expected values
///
/// The expected values are determined as follows:
///
/// Crypto-pair (retrieve ICP/BTC rate)
/// 0. The XRC retrieves the ICP/USDT rates from the mock exchange responses.
/// 1. The XRC retrieves the BTC/USDT rates from the mock exchange responses.
/// 2. The XRC divides ICP/USDT by BTC/USDT (inverting BTC/USDT to USDT/BTC and multiplying) to get the ICP/BTC rates.
/// 3. The XRC returns the median rate and the standard deviation of the ICP/BTC rates.
///    The concrete expected median rate and standard deviation are asserted below.
///
/// Crypto-fiat pair (retrieve BTC/EUR rate)
/// 0. The XRC retrieves rates from the mock forex sources and normalizes them to USD,
///    collecting the EUR/USD rate (see xrc/forex.rs).
/// 1. The XRC retrieves the BTC/USDT rates from the mock exchange responses.
/// 2. The XRC retrieves the stablecoin rates (USDS and USDC, each quoted in USDT) from the mock exchanges.
/// 3. The XRC determines the USDT/USD rate.
/// 4. The XRC multiplies the USDT/USD rate with the BTC/USDT rate to get the BTC/USD rate.
/// 5. The XRC divides BTC/USD by the forex rate EUR/USD (inverting EUR/USD to USD/EUR and multiplying) to get BTC/EUR.
/// 6. The XRC returns the median rate and the standard deviation of the BTC/EUR rates.
///    The concrete expected median rate and standard deviation are asserted below.
///
/// Fiat-crypto pair (retrieve EUR/BTC rate)
/// 0. The instructions are similar to the crypto-fiat pair. The only difference is that the rates are inverted before
///    being returned. The concrete expected median rate and standard deviation are asserted below.
///
/// Fiat pair (retrieve EUR/JPY rate)
/// 0. The XRC retrieves rates from the mock forex sources and normalizes them to USD,
///    collecting the EUR/USD and JPY/USD rates (see xrc/forex.rs).
/// 1. The XRC divides EUR/USD by JPY/USD (inverting JPY/USD to USD/JPY and multiplying) to get the EUR/JPY rates.
/// 2. The XRC returns the median rate and the standard deviation of the EUR/JPY rates.
///    The concrete expected median rate and standard deviation are asserted below.
#[ignore]
#[test]
fn basic_exchange_rates() {
    let now_seconds = time::OffsetDateTime::now_utc().unix_timestamp() as u64;
    let yesterday_timestamp_seconds = now_seconds
        .saturating_sub(ONE_DAY_SECONDS)
        .saturating_div(ONE_DAY_SECONDS)
        .saturating_mul(ONE_DAY_SECONDS);
    let timestamp_seconds = now_seconds / 60 * 60;

    let responses = mock_responses::exchanges::build_responses(
        "ICP".to_string(),
        timestamp_seconds,
        |exchange| match exchange {
            xrc::Exchange::Coinbase(_) => Some("3.92"),
            xrc::Exchange::KuCoin(_) => Some("3.92"),
            xrc::Exchange::Okx(_) => Some("3.90"),
            xrc::Exchange::GateIo(_) => Some("3.90"),
            xrc::Exchange::Mexc(_) => Some("3.911"),
            xrc::Exchange::Poloniex(_) => Some("4.005"),
            xrc::Exchange::CryptoCom(_) => Some("3.91"),
            xrc::Exchange::Bitget(_) => Some("3.93"),
            xrc::Exchange::Digifinex(_) => Some("4.00"),
        },
    )
    .chain(mock_responses::exchanges::build_common_responses(
        "BTC".to_string(),
        timestamp_seconds,
    ))
    .chain(mock_responses::stablecoin::build_responses(
        timestamp_seconds,
    ))
    .chain(mock_responses::forex::build_common_responses(now_seconds))
    .collect::<Vec<_>>();

    let env = XrcTestEnv::setup(responses, now_seconds);

    // Crypto pair
    let crypto_pair_request = GetExchangeRateRequest {
        timestamp: Some(timestamp_seconds),
        base_asset: Asset {
            symbol: "ICP".to_string(),
            class: AssetClass::Cryptocurrency,
        },
        quote_asset: Asset {
            symbol: "BTC".to_string(),
            class: AssetClass::Cryptocurrency,
        },
    };
    let exchange_rate = env
        .call_get_exchange_rate(&crypto_pair_request)
        .expect("Failed to retrieve an exchange rate from the canister.");
    assert_eq!(exchange_rate.base_asset, crypto_pair_request.base_asset);
    assert_eq!(exchange_rate.quote_asset, crypto_pair_request.quote_asset);
    assert_eq!(exchange_rate.timestamp, timestamp_seconds);
    assert_eq!(exchange_rate.metadata.base_asset_num_queried_sources, 9);
    assert_eq!(exchange_rate.metadata.base_asset_num_received_rates, 9);
    assert_eq!(exchange_rate.metadata.quote_asset_num_queried_sources, 9);
    assert_eq!(exchange_rate.metadata.quote_asset_num_received_rates, 9);
    assert_eq!(exchange_rate.metadata.standard_deviation, 3_178_330);
    assert_eq!(exchange_rate.rate, 88_813_559);

    // Crypto-fiat pair
    let crypto_fiat_pair_request = GetExchangeRateRequest {
        timestamp: Some(timestamp_seconds),
        base_asset: Asset {
            symbol: "BTC".to_string(),
            class: AssetClass::Cryptocurrency,
        },
        quote_asset: Asset {
            symbol: "EUR".to_string(),
            class: AssetClass::FiatCurrency,
        },
    };
    let exchange_rate = env
        .call_get_exchange_rate(&crypto_fiat_pair_request)
        .expect("Failed to retrieve an exchange rate from the canister.");
    assert_eq!(
        exchange_rate.base_asset,
        crypto_fiat_pair_request.base_asset
    );
    assert_eq!(
        exchange_rate.quote_asset,
        crypto_fiat_pair_request.quote_asset
    );
    assert_eq!(exchange_rate.timestamp, timestamp_seconds);
    assert_eq!(
        exchange_rate.metadata.base_asset_num_queried_sources,
        NUM_EXCHANGES
    );
    assert_eq!(
        exchange_rate.metadata.base_asset_num_received_rates,
        NUM_EXCHANGES
    );
    assert_eq!(
        exchange_rate.metadata.quote_asset_num_queried_sources,
        NUM_FOREX_SOURCES
    );
    assert_eq!(
        exchange_rate.metadata.quote_asset_num_received_rates,
        NUM_FOREX_SOURCES
    );
    assert_eq!(exchange_rate.metadata.standard_deviation, 2_138_631_519);
    assert_eq!(exchange_rate.rate, 42_522_454_766);

    // Fiat-crypto pair
    let fiat_crypto_pair_request = GetExchangeRateRequest {
        timestamp: Some(timestamp_seconds),
        base_asset: Asset {
            symbol: "EUR".to_string(),
            class: AssetClass::FiatCurrency,
        },
        quote_asset: Asset {
            symbol: "BTC".to_string(),
            class: AssetClass::Cryptocurrency,
        },
    };
    let exchange_rate = env
        .call_get_exchange_rate(&fiat_crypto_pair_request)
        .expect("Failed to retrieve an exchange rate from the canister.");
    assert_eq!(
        exchange_rate.base_asset,
        fiat_crypto_pair_request.base_asset
    );
    assert_eq!(
        exchange_rate.quote_asset,
        fiat_crypto_pair_request.quote_asset
    );
    assert_eq!(exchange_rate.timestamp, timestamp_seconds);
    assert_eq!(
        exchange_rate.metadata.base_asset_num_queried_sources,
        NUM_FOREX_SOURCES
    );
    assert_eq!(
        exchange_rate.metadata.base_asset_num_received_rates,
        NUM_FOREX_SOURCES
    );
    assert_eq!(
        exchange_rate.metadata.quote_asset_num_queried_sources,
        NUM_EXCHANGES
    );
    assert_eq!(
        exchange_rate.metadata.quote_asset_num_received_rates,
        NUM_EXCHANGES
    );
    assert_eq!(exchange_rate.metadata.standard_deviation, 1_169_238);
    assert_eq!(exchange_rate.rate, 23_516_986);

    // Fiat-pair
    let fiat_pair_request = GetExchangeRateRequest {
        timestamp: Some(timestamp_seconds),
        base_asset: Asset {
            symbol: "EUR".to_string(),
            class: AssetClass::FiatCurrency,
        },
        quote_asset: Asset {
            symbol: "JPY".to_string(),
            class: AssetClass::FiatCurrency,
        },
    };
    let exchange_rate = env
        .call_get_exchange_rate(&fiat_pair_request)
        .expect("Failed to retrieve an exchange rate from the canister.");
    assert_eq!(exchange_rate.base_asset, fiat_pair_request.base_asset);
    assert_eq!(exchange_rate.quote_asset, fiat_pair_request.quote_asset);
    assert_eq!(exchange_rate.timestamp, yesterday_timestamp_seconds);
    assert_eq!(
        exchange_rate.metadata.base_asset_num_queried_sources,
        NUM_FOREX_SOURCES
    );
    assert_eq!(
        exchange_rate.metadata.base_asset_num_received_rates,
        NUM_FOREX_SOURCES
    );
    assert_eq!(
        exchange_rate.metadata.quote_asset_num_queried_sources,
        NUM_FOREX_SOURCES
    );
    assert_eq!(
        exchange_rate.metadata.quote_asset_num_received_rates,
        NUM_FOREX_SOURCES
    );
    assert_eq!(exchange_rate.metadata.standard_deviation, 5_961_395_353);
    assert_eq!(exchange_rate.rate, 143_426_548_595);
}
