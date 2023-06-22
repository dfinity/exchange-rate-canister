use std::collections::HashMap;

use crate::{
    container::{run_scenario, Container},
    mock_responses,
};
use ic_xrc_types::{
    Asset, AssetClass, ExchangeRateError, GetExchangeRateRequest, GetExchangeRateResult,
};

/// Setup:
/// * Deploy mock FOREX data providers and exchanges, all configured to be malicious
/// * Start replicas and deploy the XRC, configured to use the mock data sources
///
/// Runbook:
/// * Request exchange rate for various cryptocurrency and fiat currency pairs
/// * Assert that errors are returned indicating that no rates could be determined
///
/// Success criteria:
/// * All queries return the expected values
///
/// The expected values are determined as follows:
///
/// Crypto-pair (retrieve ICP/BTC rate)
/// 0. Attempt to retrieve ICP/BTC rate
///     a. The XRC attempts to retrieve the ICP/USDT rate, but fails as the exchanges are not returning responses at all.
///     b. The XRC returns a `CryptoBaseAssetNotFound` error.
/// 0. Attempt to retrieve BTC/ICP rate
///     a. The XRC retrieves the BTC/USDT rate.
///     a. The XRC attempts to retrieve the ICP/USDT rate, but fails as the exchanges are not returning responses at all.
///     b. The XRC returns a `CryptoQuoteAssetNotFound` error.
/// Crypto-fiat pair (retrieve BTC/EUR rate)
/// 0. The XRC retrieves rates from the mock forex sources.
/// 1. The XRC retrieves the BTC/USDT rates from the mock exchange responses.
/// 2. The XRC attempts to retrieve the stablecoin rates from the mock exchanges, but fails to get any rates.
/// 3. The XRC returns a `StablecoinRateTooFewRates` error.
/// Fiat-crypto pair (retrieve EUR/BTC rate)
/// 0. The XRC retrieves rates from the mock forex sources.
/// 1. The XRC retrieves the BTC/USDT rates from the mock exchange responses.
/// 2. The XRC attempts to retrieve the stablecoin rates from the mock exchanges, but fails to get any rates.
/// 3. The XRC returns a `StablecoinRateTooFewRates` error.
/// Fiat pair
/// 0. Attempt to retrieve EUR/NOK rate
///     a. The XRC retrieves rates from the mock forex sources.
///         i. During collection the rates retrieved are normalized to USD.
///     b. The XRC pulls the EUR rate and attempts to pull the NOK (Norway) rate. The NOK rate does not exist in the data set.
///     c. The XRC returns a `ForexQuoteAssetNotFound` error.
/// 1. Attempt to retrieve NOK/EUR rate
///     a. The XRC retrieves rates from the mock forex sources.
///         i. During collection the rates retrieved are normalized to USD.
///     b. The XRC pulls the EUR rate and attempts to pull the NOK (Norway) rate. The NOK rate does not exist in the data set.
///     c. The XRC returns a `ForexBaseAssetNotFound` error.
#[ignore]
#[test]
fn determinism() {
    let now_seconds = time::OffsetDateTime::now_utc().unix_timestamp() as u64;
    let timestamp_seconds = now_seconds / 60 * 60;

    let responses =
        mock_responses::exchanges::build_responses("ICP".to_string(), timestamp_seconds, |_| None)
            .chain(mock_responses::exchanges::build_common_responses(
                "BTC".to_string(),
                timestamp_seconds,
            ))
            .chain(mock_responses::forex::build_responses(now_seconds, |_| {
                Some(HashMap::new())
            }))
            .collect::<Vec<_>>();

    let container = Container::builder()
        .name("determinism")
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

        // Crypto Pairs
        let icp_asset = Asset {
            symbol: "ICP".to_string(),
            class: AssetClass::Cryptocurrency,
        };

        let crypto_pair_request = GetExchangeRateRequest {
            timestamp: Some(timestamp_seconds),
            base_asset: icp_asset.clone(),
            quote_asset: btc_asset.clone(),
        };

        let crypto_pair_result = container
            .call_canister::<_, GetExchangeRateResult>("get_exchange_rate", &crypto_pair_request)
            .expect("Failed to call canister for rates");

        assert!(matches!(
            crypto_pair_result,
            Err(ExchangeRateError::CryptoBaseAssetNotFound)
        ));

        let crypto_pair_request = GetExchangeRateRequest {
            timestamp: Some(timestamp_seconds),
            base_asset: btc_asset.clone(),
            quote_asset: icp_asset,
        };

        let crypto_pair_result = container
            .call_canister::<_, GetExchangeRateResult>("get_exchange_rate", &crypto_pair_request)
            .expect("Failed to call canister for rates");

        assert!(matches!(
            crypto_pair_result,
            Err(ExchangeRateError::CryptoQuoteAssetNotFound)
        ));

        // Crypto Fiat Pair
        let crypto_fiat_pair_request = GetExchangeRateRequest {
            timestamp: Some(timestamp_seconds),
            base_asset: btc_asset.clone(),
            quote_asset: eur_asset.clone(),
        };

        let crypto_fiat_pair_result = container
            .call_canister::<_, GetExchangeRateResult>(
                "get_exchange_rate",
                &crypto_fiat_pair_request,
            )
            .expect("Failed to call canister for rates");

        assert!(matches!(
            crypto_fiat_pair_result,
            Err(ExchangeRateError::StablecoinRateTooFewRates)
        ));

        // Fiat Crypto Pair
        let fiat_crypto_pair_request = GetExchangeRateRequest {
            timestamp: Some(timestamp_seconds),
            base_asset: eur_asset.clone(),
            quote_asset: btc_asset,
        };

        let fiat_crypto_pair_result = container
            .call_canister::<_, GetExchangeRateResult>(
                "get_exchange_rate",
                &fiat_crypto_pair_request,
            )
            .expect("Failed to call canister for rates");

        assert!(matches!(
            fiat_crypto_pair_result,
            Err(ExchangeRateError::StablecoinRateTooFewRates)
        ));

        // Fiat Pair
        let nok_asset = Asset {
            symbol: "NOK".to_string(), // Norway is not in the test dataset.
            class: AssetClass::FiatCurrency,
        };

        let fiat_pair_request = GetExchangeRateRequest {
            timestamp: Some(timestamp_seconds),
            base_asset: eur_asset.clone(),
            quote_asset: nok_asset.clone(),
        };

        let fiat_pair_result = container
            .call_canister::<_, GetExchangeRateResult>("get_exchange_rate", &fiat_pair_request)
            .expect("Failed to call canister for rates");

        assert!(matches!(
            fiat_pair_result,
            Err(ExchangeRateError::ForexQuoteAssetNotFound)
        ));

        let fiat_pair_request = GetExchangeRateRequest {
            timestamp: Some(timestamp_seconds),
            base_asset: nok_asset,
            quote_asset: eur_asset,
        };

        let fiat_pair_result = container
            .call_canister::<_, GetExchangeRateResult>("get_exchange_rate", &fiat_pair_request)
            .expect("Failed to call canister for rates");

        assert!(matches!(
            fiat_pair_result,
            Err(ExchangeRateError::ForexBaseAssetNotFound)
        ));

        Ok(())
    })
    .expect("Scenario failed");
}
