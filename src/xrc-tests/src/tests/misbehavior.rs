use std::collections::HashMap;

use ic_xrc_types::{
    Asset, AssetClass, ExchangeRate, ExchangeRateMetadata, GetExchangeRateRequest,
    GetExchangeRateResult,
};
use maplit::hashmap;

use crate::tests::{NUM_EXCHANGES, NUM_FOREX_SOURCES};
use crate::{
    container::{run_scenario, Container},
    mock_responses, ONE_DAY_SECONDS,
};

/// This value is derived in the basic_exchange_rates crypto pair portion of the test.
const CRYPTO_PAIR_BASIC_STD_DEV: u64 = 3_178_330;

/// This value is derived in the basic_exchange_rates crypto fiat pair portion of the test.
const CRYPTO_FIAT_PAIR_BASIC_STD_DEV: u64 = 2_081_634_467;

/// This value is derived in the basic_exchange_rates fiat crypto pair portion of the test.
const FIAT_CRYPTO_PAIR_BASIC_STD_DEV: u64 = 1_142_788;

/// This value is derived from using the common mock dataset (mock_responses::forex::build_common_responses).
/// A full explanation how on the number is derived can be seen starting in the basic_exchange_rate test on line
/// 119.
const FIAT_PAIR_COMMON_DATASET_STD_DEV: u64 = 396_623_626;

/// Setup:
/// * Deploy mock FOREX data providers and exchanges, some of which are configured to be malicious
/// * Start replicas and deploy the XRC, configured to use the mock data sources
///
/// Runbook:
/// * Request exchange rate for various cryptocurrency and fiat currency pairs
/// * Assert that the returned rates correspond to the expected values and that the confidence is lower due to the erroneous responses
///
/// Success criteria:
/// * All queries return the expected values
///
/// The expected values are determined as follows:
///
/// Crypto-pair (retrieve ICP/BTC rate)
/// 0. The XRC retrieves the ICP/USDT and BTC/USDT rates from the mock exchange responses;
///    rates that differ from the median by 20% or more are filtered out.
/// 1. The XRC divides ICP/USDT by BTC/USDT (inverting BTC/USDT to USDT/BTC and multiplying) to get the ICP/BTC rates.
/// 2. The XRC returns the median rate and the standard deviation of the ICP/BTC rates.
///    The concrete expected median rate and standard deviation are asserted below.
///
/// Crypto-fiat pair (retrieve BTC/EUR rate)
/// 0. The XRC retrieves rates from the mock forex sources and normalizes them to USD,
///    collecting the EUR/USD rate (see xrc/forex.rs).
/// 1. The XRC retrieves the BTC/USDT rates from the mock exchange responses (outliers filtered as above).
/// 2. The XRC retrieves the stablecoin rates (USDS and USDC, each quoted in USDT) and determines the USDT/USD rate.
/// 3. The XRC multiplies USDT/USD by BTC/USDT to get BTC/USD.
/// 4. The XRC divides BTC/USD by the forex rate EUR/USD (inverting EUR/USD to USD/EUR and multiplying) to get BTC/EUR.
/// 5. The XRC returns the median rate and the standard deviation of the BTC/EUR rates.
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
fn misbehavior() {
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
            xrc::Exchange::Coinbase(_) => Some("100000.92"),
            xrc::Exchange::KuCoin(_) => Some("0.0000000001"),
            xrc::Exchange::Okx(_) => Some("3.90"),
            xrc::Exchange::GateIo(_) => Some("3.90"),
            xrc::Exchange::Mexc(_) => Some("3.911"),
            xrc::Exchange::Poloniex(_) => Some("4.005"),
            xrc::Exchange::CryptoCom(_) => Some("100000.0"),
            xrc::Exchange::Bitget(_) => Some("3.93"),
            xrc::Exchange::Digifinex(_) => Some("1000.00"),
        },
    )
    .chain(mock_responses::exchanges::build_responses(
        "BTC".to_string(),
        timestamp_seconds,
        |exchange| match exchange {
            xrc::Exchange::Coinbase(_) => Some("10000.25"),
            xrc::Exchange::KuCoin(_) => Some("10000.833"),
            xrc::Exchange::Okx(_) => Some("42.03"),
            xrc::Exchange::GateIo(_) => Some("42.64"),
            xrc::Exchange::Mexc(_) => Some("46.101"),
            xrc::Exchange::Poloniex(_) => Some("46.022"),
            xrc::Exchange::CryptoCom(_) => Some("10000.96000000"),
            xrc::Exchange::Bitget(_) => Some("45.00"),
            xrc::Exchange::Digifinex(_) => Some("1000.50")
        },
    ))
    .chain(mock_responses::stablecoin::build_responses(
        timestamp_seconds,
    ))
    .chain(mock_responses::forex::build_responses(
        now_seconds,
        |forex| match forex {
            xrc::Forex::CentralBankOfMyanmar(_) => {
                Some(hashmap! { "EUR" => "1.0", "JPY" => "10000.0" })
            }
            xrc::Forex::BankOfCanada(_) => Some(hashmap! { "EUR" => "50.0", "JPY" => "0.10" }),
            xrc::Forex::ReserveBankOfAustralia(_) => {
                Some(hashmap! { "EUR" => "10.0", "JPY" => "200.0" })
            }
            xrc::Forex::SwissFederalOfficeForCustoms(_) => {
                Some(hashmap! { "EUR" => "5.00", "JPY" => "1.00" })
            }
            xrc::Forex::CentralBankOfGeorgia(_) => {
                Some(hashmap! { "EUR" => "1000.0", "JPY" => "1000.0" })
            }
            _ => Some(HashMap::new()),
        },
    ))
    .collect::<Vec<_>>();

    let container = Container::builder()
        .name("misbehavior")
        .exchange_responses(responses)
        .build();

    run_scenario(container, |container| {
        let btc_asset = Asset {
            symbol: "BTC".to_string(),
            class: AssetClass::Cryptocurrency,
        };

        let eur_asset = Asset {
            symbol: "EUR".to_string(),
            class: AssetClass::FiatCurrency,
        };

        // Crypto Pair
        let crypto_pair_request = GetExchangeRateRequest {
            timestamp: Some(timestamp_seconds),
            base_asset: Asset {
                symbol: "ICP".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            quote_asset: btc_asset.clone(),
        };
        let expected_crypto_pair_rate = ExchangeRate {
            base_asset: Asset {
                symbol: "ICP".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            quote_asset: btc_asset.clone(),
            timestamp: timestamp_seconds,
            rate: 87_023_595,
            metadata: ExchangeRateMetadata {
                decimals: 9,
                base_asset_num_queried_sources: NUM_EXCHANGES,
                // KuCoin quotes ICP at 0.0000000001, which scales below the
                // representable resolution: it is rejected at parse and never
                // counted as a received rate, so one fewer than the other ICP
                // sources. The rate and std-dev are unchanged because that quote
                // was excluded from the median either way.
                base_asset_num_received_rates: NUM_EXCHANGES - 1,
                quote_asset_num_queried_sources: NUM_EXCHANGES,
                quote_asset_num_received_rates: NUM_EXCHANGES,
                standard_deviation: 3_644_799,
                forex_timestamp: None,
            },
        };

        let crypto_pair_result = container
            .call_canister::<_, GetExchangeRateResult>("get_exchange_rate", &crypto_pair_request)
            .expect("Failed to call canister for rates");
        let exchange_rate =
            crypto_pair_result.expect("Failed to retrieve an exchange rate from the canister.");

        assert_eq!(exchange_rate, expected_crypto_pair_rate);
        assert!(CRYPTO_PAIR_BASIC_STD_DEV < exchange_rate.metadata.standard_deviation);

        // Crypto Fiat Pair
        let crypto_fiat_pair_request = GetExchangeRateRequest {
            timestamp: Some(timestamp_seconds),
            base_asset: btc_asset.clone(),
            quote_asset: eur_asset.clone(),
        };
        let expected_crypto_fiat_pair_rate = ExchangeRate {
            base_asset: btc_asset.clone(),
            quote_asset: eur_asset.clone(),
            timestamp: timestamp_seconds,
            rate: 42_742_607_798,
            metadata: ExchangeRateMetadata {
                decimals: 9,
                base_asset_num_queried_sources: NUM_EXCHANGES,
                base_asset_num_received_rates: NUM_EXCHANGES,
                quote_asset_num_queried_sources: NUM_FOREX_SOURCES,
                quote_asset_num_received_rates: NUM_FOREX_SOURCES,
                standard_deviation: 2_408_021_784,
                forex_timestamp: Some(yesterday_timestamp_seconds),
            },
        };

        let crypto_fiat_pair_result = container
            .call_canister::<_, GetExchangeRateResult>(
                "get_exchange_rate",
                &crypto_fiat_pair_request,
            )
            .expect("Failed to call canister for rates");
        let exchange_rate = crypto_fiat_pair_result
            .expect("Failed to retrieve an exchange rate from the canister.");

        assert_eq!(exchange_rate, expected_crypto_fiat_pair_rate);
        assert!(CRYPTO_FIAT_PAIR_BASIC_STD_DEV < exchange_rate.metadata.standard_deviation);

        // Fiat Crypto Pair
        let fiat_crypto_pair_request = GetExchangeRateRequest {
            timestamp: Some(timestamp_seconds),
            base_asset: eur_asset.clone(),
            quote_asset: btc_asset.clone(),
        };
        let expected_fiat_crypto_pair_rate = ExchangeRate {
            base_asset: eur_asset.clone(),
            quote_asset: btc_asset,
            timestamp: timestamp_seconds,
            rate: 23_395_859,
            metadata: ExchangeRateMetadata {
                decimals: 9,
                base_asset_num_queried_sources: NUM_FOREX_SOURCES,
                base_asset_num_received_rates: NUM_FOREX_SOURCES,
                quote_asset_num_queried_sources: NUM_EXCHANGES,
                quote_asset_num_received_rates: NUM_EXCHANGES,
                standard_deviation: 1_304_018,
                forex_timestamp: Some(yesterday_timestamp_seconds),
            },
        };

        let fiat_crypto_pair_result = container
            .call_canister::<_, GetExchangeRateResult>(
                "get_exchange_rate",
                &fiat_crypto_pair_request,
            )
            .expect("Failed to call canister for rates");
        let exchange_rate = fiat_crypto_pair_result
            .expect("Failed to retrieve an exchange rate from the canister.");

        assert_eq!(exchange_rate, expected_fiat_crypto_pair_rate);
        assert!(FIAT_CRYPTO_PAIR_BASIC_STD_DEV < exchange_rate.metadata.standard_deviation);

        // Fiat Pair
        let fiat_pair_request = GetExchangeRateRequest {
            timestamp: Some(timestamp_seconds),
            base_asset: eur_asset.clone(),
            quote_asset: Asset {
                symbol: "JPY".to_string(),
                class: AssetClass::FiatCurrency,
            },
        };

        let expected_fiat_pair_rate = ExchangeRate {
            base_asset: eur_asset,
            quote_asset: Asset {
                symbol: "JPY".to_string(),
                class: AssetClass::FiatCurrency,
            },
            timestamp: yesterday_timestamp_seconds,
            rate: 143_819_688_082,
            metadata: ExchangeRateMetadata {
                decimals: 9,
                base_asset_num_queried_sources: NUM_FOREX_SOURCES,
                base_asset_num_received_rates: NUM_FOREX_SOURCES,
                quote_asset_num_queried_sources: NUM_FOREX_SOURCES,
                quote_asset_num_received_rates: NUM_FOREX_SOURCES,
                standard_deviation: 7_313_975_259,
                forex_timestamp: Some(yesterday_timestamp_seconds),
            },
        };

        let fiat_pair_result = container
            .call_canister::<_, GetExchangeRateResult>("get_exchange_rate", &fiat_pair_request)
            .expect("Failed to call canister for rates");
        let exchange_rate =
            fiat_pair_result.expect("Failed to retrieve an exchange rate from the canister.");
        assert_eq!(exchange_rate, expected_fiat_pair_rate);
        assert!(FIAT_PAIR_COMMON_DATASET_STD_DEV < exchange_rate.metadata.standard_deviation);

        Ok(())
    })
    .expect("Scenario failed");
}
