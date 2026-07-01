use std::{
    collections::{BTreeMap, BTreeSet},
    sync::RwLock,
};

use async_trait::async_trait;
use futures::FutureExt;
use ic_xrc_types::{Asset, AssetClass, ExchangeRateError, GetExchangeRateRequest};
use maplit::btreemap;

use crate::{
    environment::test::TestEnvironment,
    exchanges::{Coinbase, ListedPairs},
    forex::COMPUTED_XDR_SYMBOL,
    inflight::test::set_inflight_tracking,
    rate_limiting::test::{set_request_counter, REQUEST_COUNTER_TRIGGER_RATE_LIMIT},
    usdt_asset, with_cache_mut, with_forex_rate_store_mut, with_listing_store_mut,
    CallExchangeError, Exchange,
    QueriedExchangeRate, EXCHANGES, PRIVILEGED_CANISTER_IDS, RATE_UNIT, USDC, USDS,
    XRC_BASE_CYCLES_COST, XRC_IMMEDIATE_REFUND_CYCLES, XRC_MINIMUM_FEE_COST,
    XRC_OUTBOUND_HTTP_CALL_CYCLES_COST, XRC_REQUEST_CYCLES_COST,
};

use super::{
    aggregate_cryptocurrency_usdt_rates, get_exchange_rate_internal, usd_asset, CallExchanges,
    QueriedExchangeRateWithFailedExchanges,
};

/// The function returns the Euro asset.
pub(crate) fn eur_asset() -> Asset {
    Asset {
        symbol: "EUR".to_string(),
        class: AssetClass::FiatCurrency,
    }
}

/// The function returns the ICP utility token.
pub(crate) fn icp_asset() -> Asset {
    Asset {
        symbol: "ICP".to_string(),
        class: AssetClass::Cryptocurrency,
    }
}

/// The function returns the Bitcoin asset.
pub(crate) fn btc_asset() -> Asset {
    Asset {
        symbol: "BTC".to_string(),
        class: AssetClass::Cryptocurrency,
    }
}

/// The function returns the PEPE (crypto) asset.
pub(crate) fn pepe_asset() -> Asset {
    Asset {
        symbol: "PEPE".to_string(),
        class: AssetClass::Cryptocurrency,
    }
}

/// The function returns the British Pound asset.
pub(crate) fn gbp_asset() -> Asset {
    Asset {
        symbol: "GBP".to_string(),
        class: AssetClass::FiatCurrency,
    }
}

fn test_cxdr_rate() -> QueriedExchangeRate {
    QueriedExchangeRate::new(
        Asset {
            symbol: COMPUTED_XDR_SYMBOL.to_string(),
            class: AssetClass::FiatCurrency,
        },
        usd_asset(),
        0,
        &[800_000_000, 800_000_000, 800_000_000, 800_000_000],
        4,
        4,
        Some(0),
    )
}

/// Used to simulate HTTP outcalls from the canister for testing purposes.
#[derive(Default)]
struct TestCallExchangesImpl {
    /// Contains the responses when [CallExchanges::get_cryptocurrency_usdt_rate] is called.
    get_cryptocurrency_usdt_rate_responses:
        BTreeMap<String, Result<QueriedExchangeRateWithFailedExchanges, CallExchangeError>>,
    /// The received [CallExchanges::get_cryptocurrency_usdt_rate] calls from the test.
    get_cryptocurrency_usdt_rate_calls: RwLock<Vec<(Vec<Exchange>, Asset, u64)>>,
    /// Contains the responses when [CallExchanges::get_stablecoin_rates] is called.
    get_stablecoin_rates_responses:
        BTreeMap<String, Result<QueriedExchangeRateWithFailedExchanges, CallExchangeError>>,
    #[allow(clippy::type_complexity)]
    /// The received [CallExchanges::get_cryptocurrency_usdt_rate] calls from the test.
    get_stablecoin_rates_calls: RwLock<Vec<(Vec<Exchange>, Vec<String>, u64)>>,
}

impl TestCallExchangesImpl {
    fn builder() -> TestCallExchangesImplBuilder {
        TestCallExchangesImplBuilder::new()
    }
}

struct TestCallExchangesImplBuilder {
    r#impl: TestCallExchangesImpl,
}

impl TestCallExchangesImplBuilder {
    fn new() -> Self {
        Self {
            r#impl: TestCallExchangesImpl::default(),
        }
    }

    /// Sets the responses for when [CallExchanges::get_cryptocurrency_usdt_rate] is called.
    fn with_get_cryptocurrency_usdt_rate_responses(
        mut self,
        responses: BTreeMap<
            String,
            Result<QueriedExchangeRateWithFailedExchanges, CallExchangeError>,
        >,
    ) -> Self {
        self.r#impl.get_cryptocurrency_usdt_rate_responses = responses;
        self
    }

    /// Sets the responses for when [CallExchanges::get_stablecoin_rates] is called.
    fn with_get_stablecoin_rates_responses(
        mut self,
        responses: BTreeMap<
            String,
            Result<QueriedExchangeRateWithFailedExchanges, CallExchangeError>,
        >,
    ) -> Self {
        self.r#impl.get_stablecoin_rates_responses = responses;
        self
    }

    /// Returns the built implmentation.
    fn build(self) -> TestCallExchangesImpl {
        self.r#impl
    }
}

#[async_trait]
impl CallExchanges for TestCallExchangesImpl {
    async fn get_cryptocurrency_usdt_rate(
        &self,
        exchanges: &[&Exchange],
        asset: &Asset,
        timestamp: u64,
    ) -> Result<QueriedExchangeRateWithFailedExchanges, CallExchangeError> {
        let exchanges_vec = exchanges
            .iter()
            .map(|e| e.to_owned().clone())
            .collect::<Vec<_>>();
        self.get_cryptocurrency_usdt_rate_calls
            .write()
            .unwrap()
            .push((exchanges_vec, asset.clone(), timestamp));
        self.get_cryptocurrency_usdt_rate_responses
            .get(&asset.symbol)
            .cloned()
            .unwrap_or(Err(CallExchangeError::NoRatesFound))
    }

    async fn get_stablecoin_rates(
        &self,
        exchanges: &[&Exchange],
        assets: &[&str],
        timestamp: u64,
    ) -> Vec<Result<QueriedExchangeRateWithFailedExchanges, CallExchangeError>> {
        let exchanges_vec = exchanges
            .iter()
            .map(|e| e.to_owned().clone())
            .collect::<Vec<_>>();
        let assets_vec = assets.iter().map(|a| a.to_string()).collect::<Vec<_>>();
        self.get_stablecoin_rates_calls.write().unwrap().push((
            exchanges_vec,
            assets_vec,
            timestamp,
        ));

        let mut results = vec![];
        for asset in assets {
            let entry = self
                .get_stablecoin_rates_responses
                .get(*asset)
                .expect("Failed to retrieve stablecoin rate")
                .clone();
            results.push(entry);
        }

        results
    }
}

/// A simple mock BTC/USDT [QueriedExchangeRate].
fn btc_queried_exchange_rate_mock() -> QueriedExchangeRate {
    QueriedExchangeRate::new(
        btc_asset(),
        usdt_asset(),
        0,
        &[16_000 * RATE_UNIT, 16_001 * RATE_UNIT, 15_999 * RATE_UNIT],
        EXCHANGES.len(),
        3,
        None,
    )
}

/// A simple mock BTC/USDT [QueriedExchangeRateWithFailedExchanges].
fn btc_queried_exchange_rate_with_failed_exchanges_mock(
    failed_exchanges: Vec<Exchange>,
) -> QueriedExchangeRateWithFailedExchanges {
    QueriedExchangeRateWithFailedExchanges {
        queried_exchange_rate: btc_queried_exchange_rate_mock(),
        failed_exchanges,
    }
}

/// A simple mock ICP/USDT [QueriedExchangeRate].
fn icp_queried_exchange_rate_mock() -> QueriedExchangeRate {
    QueriedExchangeRate::new(
        icp_asset(),
        usdt_asset(),
        0,
        &[4 * RATE_UNIT, 4 * RATE_UNIT, 4 * RATE_UNIT],
        EXCHANGES.len(),
        3,
        None,
    )
}

/// A simple mock ICP/USDT [QueriedExchangeRateWithFailedExchanges].
fn icp_queried_exchange_rate_with_failed_exchanges_mock(
    failed_exchanges: Vec<Exchange>,
) -> QueriedExchangeRateWithFailedExchanges {
    QueriedExchangeRateWithFailedExchanges {
        queried_exchange_rate: icp_queried_exchange_rate_mock(),
        failed_exchanges,
    }
}

/// A simple mock ICP/USDT [QueriedExchangeRate] with only one rate.
fn icp_queried_exchange_rate_with_one_rate_mock() -> QueriedExchangeRate {
    QueriedExchangeRate::new(
        icp_asset(),
        usdt_asset(),
        0,
        &[8 * RATE_UNIT],
        EXCHANGES.len(),
        1,
        None,
    )
}

/// Regression test for the cache-poisoning issue: a fresh crypto request whose
/// post-filter rate set is empty must fail *and* must not leave an invalid
/// intermediate in the cache. Otherwise a later cache-only request for the same
/// asset and timestamp would be served a successful zero rate.
#[test]
fn failed_validation_does_not_poison_cache_with_zero_rate() {
    set_request_counter(0);

    let timestamp: u64 = 12_345_600;
    // A single raw rate of 0 becomes an empty vector once
    // QueriedExchangeRate::new filters out the invalid (zero) rate.
    let empty_post_filter_rate = QueriedExchangeRate::new(
        icp_asset(),
        usdt_asset(),
        timestamp,
        &[0],
        EXCHANGES.len(),
        1,
        None,
    );
    assert!(empty_post_filter_rate.rates.is_empty());

    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(btreemap! {
            "ICP".to_string() => Ok(QueriedExchangeRateWithFailedExchanges {
                queried_exchange_rate: empty_post_filter_rate,
                failed_exchanges: vec![],
            })
        })
        .build();
    let env = TestEnvironment::builder()
        .with_time_secs(timestamp)
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_BASE_CYCLES_COST + XRC_OUTBOUND_HTTP_CALL_CYCLES_COST)
        .build();
    let request = GetExchangeRateRequest {
        base_asset: icp_asset(),
        quote_asset: usdt_asset(),
        timestamp: Some(timestamp),
    };

    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");
    assert!(
        result.is_err(),
        "a request with an empty post-filter rate must fail, got: {:#?}",
        result
    );

    let cached_rate = with_cache_mut(|cache| cache.get("ICP", timestamp));
    assert!(
        cached_rate.is_none(),
        "the invalid intermediate must not be cached, got: {:#?}",
        cached_rate
    );
}

/// End-to-end check that an empty post-filter intermediate can never replay a
/// zero rate from the cache. Because such an intermediate is no longer cached,
/// a second request for the same asset and timestamp re-fetches (one outbound
/// call) instead of being served from cache; with no data available it errors
/// again rather than returning Ok(rate = 0).
#[test]
fn empty_post_filter_rate_is_not_cached_and_forces_refetch() {
    set_request_counter(0);

    let timestamp: u64 = 12_345_660;
    let empty_post_filter_rate = QueriedExchangeRate::new(
        icp_asset(),
        usdt_asset(),
        timestamp,
        &[0],
        EXCHANGES.len(),
        1,
        None,
    );

    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(btreemap! {
            "ICP".to_string() => Ok(QueriedExchangeRateWithFailedExchanges {
                queried_exchange_rate: empty_post_filter_rate,
                failed_exchanges: vec![],
            })
        })
        .build();
    let request = GetExchangeRateRequest {
        base_asset: icp_asset(),
        quote_asset: usdt_asset(),
        timestamp: Some(timestamp),
    };

    let first_env = TestEnvironment::builder()
        .with_time_secs(timestamp)
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_BASE_CYCLES_COST + XRC_OUTBOUND_HTTP_CALL_CYCLES_COST)
        .build();
    let first_result = get_exchange_rate_internal(&first_env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");
    assert!(first_result.is_err());

    // Nothing was cached, so the second request must re-fetch (one outbound
    // call) rather than be served the previously-poisoned cache entry.
    let second_call_exchanges_impl = TestCallExchangesImpl::builder().build();
    let second_env = TestEnvironment::builder()
        .with_time_secs(timestamp)
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_BASE_CYCLES_COST + XRC_OUTBOUND_HTTP_CALL_CYCLES_COST)
        .build();
    let second_result =
        get_exchange_rate_internal(&second_env, &second_call_exchanges_impl, &request)
            .now_or_never()
            .expect("future should complete");

    assert!(
        !matches!(second_result, Ok(ref rate) if rate.rate == 0),
        "cache-only request must not return a successful zero rate, got: {:#?}",
        second_result
    );
    assert!(
        matches!(second_result, Err(ExchangeRateError::CryptoBaseAssetNotFound)),
        "expected the re-fetch to fail with CryptoBaseAssetNotFound, got: {:#?}",
        second_result
    );
    assert_eq!(
        second_call_exchanges_impl
            .get_cryptocurrency_usdt_rate_calls
            .read()
            .unwrap()
            .len(),
        1,
        "the second request should have re-fetched rather than hit a poisoned cache"
    );
}

/// When every usable quote is below the representable resolution, the aggregator
/// reports it distinctly rather than as missing data.
#[test]
fn aggregate_all_below_resolution_reports_below_resolution() {
    // Match the queried-exchange count to the number of results, as in production.
    let exchanges: Vec<&Exchange> = EXCHANGES.iter().take(2).collect();
    let results = vec![
        Err(CallExchangeError::RateBelowResolution),
        Err(CallExchangeError::RateBelowResolution),
    ];
    let result = aggregate_cryptocurrency_usdt_rates(&icp_asset(), &exchanges, 0, results);
    assert!(matches!(result, Err(CallExchangeError::RateBelowResolution)));
}

/// A below-resolution quote alongside a representable one yields the
/// representable rate; the below-resolution source is simply dropped.
#[test]
fn aggregate_mixed_below_resolution_and_rate_yields_rate() {
    // Match the queried-exchange count to the number of results, as in production.
    let exchanges: Vec<&Exchange> = EXCHANGES.iter().take(2).collect();
    let results = vec![
        Ok(4 * RATE_UNIT),
        Err(CallExchangeError::RateBelowResolution),
    ];
    let result = aggregate_cryptocurrency_usdt_rates(&icp_asset(), &exchanges, 0, results);
    assert!(
        matches!(result, Ok(ref r) if r.queried_exchange_rate.rates == vec![4 * RATE_UNIT]),
        "got: {:#?}",
        result
    );
}

/// Representable but mutually inconsistent rates are all dropped by the
/// post-filter; that is missing data, not a below-resolution condition.
#[test]
fn aggregate_inconsistent_rates_report_no_rates_found() {
    // Match the queried-exchange count to the number of results, as in production.
    let exchanges: Vec<&Exchange> = EXCHANGES.iter().take(2).collect();
    let results = vec![Ok(RATE_UNIT), Ok(1000 * RATE_UNIT)];
    let result = aggregate_cryptocurrency_usdt_rates(&icp_asset(), &exchanges, 0, results);
    assert!(matches!(result, Err(CallExchangeError::NoRatesFound)));
}

/// No results at all is missing data.
#[test]
fn aggregate_no_results_reports_no_rates_found() {
    // No results, so no exchanges were queried.
    let exchanges: Vec<&Exchange> = vec![];
    let result = aggregate_cryptocurrency_usdt_rates(&icp_asset(), &exchanges, 0, vec![]);
    assert!(matches!(result, Err(CallExchangeError::NoRatesFound)));
}

/// A below-resolution quote mixed only with HTTP failures (no representable
/// rate) is still reported as below-resolution.
#[test]
fn aggregate_below_resolution_with_http_failure_reports_below_resolution() {
    // Match the queried-exchange count to the number of results, as in production.
    // Coinbase is first in EXCHANGES, so the Http failure below resolves to it.
    let exchanges: Vec<&Exchange> = EXCHANGES.iter().take(2).collect();
    let results = vec![
        Err(CallExchangeError::RateBelowResolution),
        Err(CallExchangeError::Http {
            exchange: "Coinbase".to_string(),
            error: "boom".to_string(),
        }),
    ];
    let result = aggregate_cryptocurrency_usdt_rates(&icp_asset(), &exchanges, 0, results);
    assert!(matches!(result, Err(CallExchangeError::RateBelowResolution)));
}

/// A crypto/USDT request whose base leg is below resolution returns the distinct
/// below-resolution Other error, not a generic asset-not-found error.
#[test]
fn below_resolution_base_leg_returns_below_resolution_error() {
    set_request_counter(0);

    let timestamp: u64 = 12_345_720;
    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(btreemap! {
            "ICP".to_string() => Err(CallExchangeError::RateBelowResolution)
        })
        .build();
    let env = TestEnvironment::builder()
        .with_time_secs(timestamp)
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_BASE_CYCLES_COST + XRC_OUTBOUND_HTTP_CALL_CYCLES_COST)
        .build();
    let request = GetExchangeRateRequest {
        base_asset: icp_asset(),
        quote_asset: usdt_asset(),
        timestamp: Some(timestamp),
    };

    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");
    assert!(
        matches!(
            result,
            Err(ExchangeRateError::Other(ref e))
                if e.code == crate::errors::RATE_BELOW_RESOLUTION_ERROR_CODE
        ),
        "expected the below-resolution Other error, got: {:#?}",
        result
    );
}

/// A crypto/crypto request whose quote leg is below resolution surfaces the
/// below-resolution Other error rather than CryptoQuoteAssetNotFound.
#[test]
fn below_resolution_quote_leg_returns_below_resolution_error() {
    set_request_counter(0);

    let timestamp: u64 = 12_345_780;
    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(btreemap! {
            "BTC".to_string() => Ok(btc_queried_exchange_rate_with_failed_exchanges_mock(vec![])),
            "ICP".to_string() => Err(CallExchangeError::RateBelowResolution)
        })
        .build();
    let env = TestEnvironment::builder()
        .with_time_secs(timestamp)
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_BASE_CYCLES_COST + 2 * XRC_OUTBOUND_HTTP_CALL_CYCLES_COST)
        .build();
    let request = GetExchangeRateRequest {
        base_asset: btc_asset(),
        quote_asset: icp_asset(),
        timestamp: Some(timestamp),
    };

    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");
    assert!(
        matches!(
            result,
            Err(ExchangeRateError::Other(ref e))
                if e.code == crate::errors::RATE_BELOW_RESOLUTION_ERROR_CODE
        ),
        "expected the below-resolution Other error, got: {:#?}",
        result
    );
}

/// Builds a crypto/USDT rate whose post-filter rate vector is internally
/// inconsistent. The struct is built directly rather than via
/// `QueriedExchangeRate::new` on purpose: `new` would filter the outliers out,
/// so this stands in for any rate that could reach the cache without passing
/// validation. It lets the tests below assert that the cache-only paths
/// re-validate the composed result instead of returning it unchecked.
fn inconsistent_crypto_usdt_rate_mock(base_asset: Asset) -> QueriedExchangeRate {
    QueriedExchangeRate {
        base_asset,
        quote_asset: usdt_asset(),
        timestamp: 0,
        // 1.0 / 3.0 / 9.0: the median is non-zero, but the spread is far beyond
        // the allowed deviation, so `validate` returns InconsistentRatesReceived.
        // The ratios are preserved under division by a constant, so the composed
        // rate stays inconsistent.
        rates: vec![RATE_UNIT, 3 * RATE_UNIT, 9 * RATE_UNIT],
        base_asset_num_queried_sources: EXCHANGES.len(),
        base_asset_num_received_rates: 3,
        quote_asset_num_queried_sources: EXCHANGES.len(),
        quote_asset_num_received_rates: 3,
        ..Default::default()
    }
}

/// The cache-only crypto/crypto path must re-validate the composed result,
/// mirroring the fresh path, so a cached rate that does not pass validation can
/// never be returned unchecked. BTC/USDT is inconsistent in the cache and
/// ICP/USDT is a single-rate entry, so the composed BTC/ICP rate is inconsistent
/// and the request must fail rather than return Ok.
#[test]
fn cache_only_crypto_pair_validates_the_composed_rate() {
    with_cache_mut(|cache| {
        cache.insert(&inconsistent_crypto_usdt_rate_mock(btc_asset()));
        cache.insert(&icp_queried_exchange_rate_with_one_rate_mock());
    });
    set_inflight_tracking(vec!["BTC".to_string(), "ICP".to_string()], 60);
    // An empty builder proves the result comes from the cache-only path: if it
    // tried to re-fetch instead, the request would fail with a different error.
    let call_exchanges_impl = TestCallExchangesImpl::builder().build();
    let env = TestEnvironment::builder()
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_BASE_CYCLES_COST)
        .with_time_secs(90)
        .build();
    let request = GetExchangeRateRequest {
        base_asset: btc_asset(),
        quote_asset: icp_asset(),
        timestamp: None,
    };

    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");

    assert!(
        matches!(result, Err(ExchangeRateError::InconsistentRatesReceived)),
        "the cache-only crypto/crypto path must validate the composed rate, got: {:#?}",
        result
    );
}

/// The cache-only crypto/fiat path must likewise re-validate the composed
/// result. ICP/USDT is inconsistent in the cache and the stablecoins resolve to
/// the unit rate, so the composed ICP/USD rate is inconsistent and the request
/// must fail rather than return Ok.
#[test]
fn cache_only_crypto_fiat_pair_validates_the_composed_rate() {
    with_cache_mut(|cache| {
        cache.insert(&inconsistent_crypto_usdt_rate_mock(icp_asset()));
        cache.insert(&stablecoin_mock(USDS, &[RATE_UNIT]));
        cache.insert(&stablecoin_mock(USDC, &[RATE_UNIT]));
    });
    set_inflight_tracking(vec!["ICP".to_string()], 60);
    let call_exchanges_impl = TestCallExchangesImpl::builder().build();
    let env = TestEnvironment::builder()
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_BASE_CYCLES_COST)
        .with_time_secs(90)
        .build();
    let request = GetExchangeRateRequest {
        base_asset: icp_asset(),
        quote_asset: usd_asset(),
        timestamp: None,
    };

    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");

    assert!(
        matches!(result, Err(ExchangeRateError::InconsistentRatesReceived)),
        "the cache-only crypto/fiat path must validate the composed rate, got: {:#?}",
        result
    );
}

fn stablecoin_mock(symbol: &str, rates: &[u64]) -> QueriedExchangeRate {
    QueriedExchangeRate::new(
        Asset {
            symbol: symbol.to_string(),
            class: AssetClass::Cryptocurrency,
        },
        usdt_asset(),
        0,
        rates,
        EXCHANGES.len(),
        rates.len(),
        None,
    )
}

fn stablecoin_mock_with_failed_exchanges(
    symbol: &str,
    rates: &[u64],
    failed_exchanges: Vec<Exchange>,
) -> QueriedExchangeRateWithFailedExchanges {
    QueriedExchangeRateWithFailedExchanges {
        queried_exchange_rate: stablecoin_mock(symbol, rates),
        failed_exchanges,
    }
}

/// This function tests that subsequent calls to to an exchange are not made when the first call
/// fails due to an HTTP error.
#[test]
fn get_exchange_rate_skips_exchanges_that_fail_for_cryptocurrency_usdt_rate() {
    let current_timestamp: u64 = 1678752000;
    let coinbase = Exchange::Coinbase(Coinbase);
    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(btreemap! {
            "BTC".to_string() => Ok(btc_queried_exchange_rate_with_failed_exchanges_mock(vec![coinbase.clone()])),
            "ICP".to_string() => Ok(icp_queried_exchange_rate_with_failed_exchanges_mock(vec![])),
        })
        .build();
    let env = TestEnvironment::builder()
        .with_time_secs(current_timestamp)
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_MINIMUM_FEE_COST * 500)
        .build();
    let request = GetExchangeRateRequest {
        base_asset: btc_asset(),
        quote_asset: icp_asset(),
        timestamp: None,
    };

    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");

    let calls = call_exchanges_impl
        .get_cryptocurrency_usdt_rate_calls
        .read()
        .unwrap();
    let quote_asset_called_exchanges = calls[1].0.clone();
    // Ensure that Coinbase was excluded from the second (quote asset) call.
    assert!(!quote_asset_called_exchanges.contains(&coinbase));
    assert!(result.is_ok());
    assert_eq!(
        call_exchanges_impl
            .get_cryptocurrency_usdt_rate_calls
            .read()
            .unwrap()
            .len(),
        2
    );
}

/// This function tests that subsequent calls to to an exchange are not made to obtain
/// stablecoin rates when the first call fails due to an HTTP error.
#[test]
fn get_exchange_rate_skips_exchanges_that_fail_for_stablecoin_rate() {
    let current_timestamp: u64 = 1678752000;
    let coinbase = Exchange::Coinbase(Coinbase);

    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(btreemap! {
            "ICP".to_string() => Ok(icp_queried_exchange_rate_with_failed_exchanges_mock(vec![]))
        })
        .with_get_stablecoin_rates_responses(btreemap! {
            USDS.to_string() => Ok(stablecoin_mock_with_failed_exchanges(USDS, &[RATE_UNIT], vec![coinbase.clone()])),
            USDC.to_string() => Ok(stablecoin_mock_with_failed_exchanges(USDC, &[RATE_UNIT], vec![]))
        })
        .build();

    let env = TestEnvironment::builder()
        .with_time_secs(current_timestamp)
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_MINIMUM_FEE_COST * 500)
        .build();
    let request = GetExchangeRateRequest {
        base_asset: usd_asset(),
        quote_asset: icp_asset(),
        timestamp: None,
    };

    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");

    let crypto_usdt_rate_call = call_exchanges_impl
        .get_cryptocurrency_usdt_rate_calls
        .read()
        .unwrap();
    let crypto_exchanges_called = crypto_usdt_rate_call[0].0.clone();
    // Ensure that Coinbase was excluded from the second call.
    assert!(!crypto_exchanges_called.contains(&coinbase));
    assert!(result.is_ok());
    assert_eq!(
        call_exchanges_impl
            .get_stablecoin_rates_calls
            .read()
            .unwrap()
            .len(),
        1
    );
}

/// This function tests that [get_exchange_rate] will return an [ExchangeRateError::NotEnoughCycles]
/// when not enough cycles are sent by the caller.
#[test]
fn get_exchange_rate_fails_when_not_enough_cycles() {
    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(btreemap! {
            "BTC".to_string() => Ok(btc_queried_exchange_rate_with_failed_exchanges_mock(vec![])),
            "ICP".to_string() => Ok(icp_queried_exchange_rate_with_failed_exchanges_mock(vec![]))
        })
        .build();
    let env = TestEnvironment::builder().with_cycles_available(0).build();
    let request = GetExchangeRateRequest {
        base_asset: btc_asset(),
        quote_asset: icp_asset(),
        timestamp: None,
    };

    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");
    assert!(matches!(result, Err(ExchangeRateError::NotEnoughCycles)));
}

/// This function tests that [get_exchange_rate] will trap when the canister fails to
/// accept the cycles sent by the caller.
#[test]
#[should_panic(expected = "Failed to accept cycles")]
fn get_exchange_rate_fails_when_unable_to_accept_cycles() {
    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(btreemap! {
            "BTC".to_string() => Ok(btc_queried_exchange_rate_with_failed_exchanges_mock(vec![])),
            "ICP".to_string() => Ok(icp_queried_exchange_rate_with_failed_exchanges_mock(vec![]))
        })
        .build();
    let env = TestEnvironment::builder()
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(0)
        .build();
    let request = GetExchangeRateRequest {
        base_asset: eur_asset(),
        quote_asset: usd_asset(),
        timestamp: None,
    };

    get_exchange_rate_internal(&env, &call_exchanges_impl, &request).now_or_never();
}

/// This function tests that [get_exchange_rate] does not charge the cycles minting canister for usage.
#[test]
fn get_exchange_rate_will_not_charge_cycles_if_caller_is_privileged() {
    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(btreemap! {
            "BTC".to_string() => Ok(btc_queried_exchange_rate_with_failed_exchanges_mock(vec![])),
            "ICP".to_string() => Ok(icp_queried_exchange_rate_with_failed_exchanges_mock(vec![]))
        })
        .build();
    let env = TestEnvironment::builder()
        .with_cycles_available(0)
        .with_caller(PRIVILEGED_CANISTER_IDS[0])
        .build();
    let request = GetExchangeRateRequest {
        base_asset: btc_asset(),
        quote_asset: icp_asset(),
        timestamp: Some(0),
    };

    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");
    assert!(result.is_ok());
    assert_eq!(
        call_exchanges_impl
            .get_cryptocurrency_usdt_rate_calls
            .read()
            .unwrap()
            .len(),
        2
    );
}

/// This function tests that [get_exchange_rate] charges the full cycles fee for usage when the cache does not
/// contain the necessary entries.
#[test]
fn get_exchange_rate_will_charge_cycles() {
    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(btreemap! {
            "BTC".to_string() => Ok(btc_queried_exchange_rate_with_failed_exchanges_mock(vec![])),
            "ICP".to_string() => Ok(icp_queried_exchange_rate_with_failed_exchanges_mock(vec![]))
        })
        .build();
    let env = TestEnvironment::builder()
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_IMMEDIATE_REFUND_CYCLES)
        .build();
    let request = GetExchangeRateRequest {
        base_asset: btc_asset(),
        quote_asset: icp_asset(),
        timestamp: Some(0),
    };

    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");
    assert!(result.is_ok());
    assert_eq!(
        call_exchanges_impl
            .get_cryptocurrency_usdt_rate_calls
            .read()
            .unwrap()
            .len(),
        2
    );
}

/// This function tests that [get_exchange_rate] charges the base cycles cost for usage.
#[test]
fn get_exchange_rate_will_charge_the_base_cost_worth_of_cycles() {
    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(btreemap! {
            "BTC".to_string() => Ok(btc_queried_exchange_rate_with_failed_exchanges_mock(vec![])),
            "ICP".to_string() => Ok(icp_queried_exchange_rate_with_failed_exchanges_mock(vec![]))
        })
        .build();
    let env = TestEnvironment::builder()
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_BASE_CYCLES_COST)
        .with_time_secs(100)
        .build();
    with_cache_mut(|cache| {
        cache.insert(&btc_queried_exchange_rate_mock());
        cache.insert(&icp_queried_exchange_rate_mock());
    });

    let request = GetExchangeRateRequest {
        base_asset: btc_asset(),
        quote_asset: icp_asset(),
        timestamp: Some(0),
    };

    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");
    assert!(result.is_ok());
    assert_eq!(
        call_exchanges_impl
            .get_cryptocurrency_usdt_rate_calls
            .read()
            .unwrap()
            .len(),
        0
    );
}

/// This function tests that [get_exchange_rate] charges the base cycles cost plus the cost of a single exchange rate lookup when there
/// is only one entry found in the cache.
#[test]
fn get_exchange_rate_will_charge_the_base_cost_plus_outbound_cycles_worth_of_cycles_when_cache_contains_one_entry(
) {
    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(btreemap! {
            "BTC".to_string() => Ok(btc_queried_exchange_rate_with_failed_exchanges_mock(vec![])),
            "ICP".to_string() => Ok(icp_queried_exchange_rate_with_failed_exchanges_mock(vec![]))
        })
        .build();
    let env = TestEnvironment::builder()
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_BASE_CYCLES_COST + XRC_OUTBOUND_HTTP_CALL_CYCLES_COST)
        .with_time_secs(100)
        .build();
    with_cache_mut(|cache| {
        cache.insert(&btc_queried_exchange_rate_mock());
    });

    let request = GetExchangeRateRequest {
        base_asset: btc_asset(),
        quote_asset: icp_asset(),
        timestamp: Some(0),
    };

    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");
    assert!(result.is_ok());
    assert_eq!(
        call_exchanges_impl
            .get_cryptocurrency_usdt_rate_calls
            .read()
            .unwrap()
            .len(),
        1
    );
}

/// This function tests that [get_exchange_rate] charges the rate limit fee for usage when there are too many HTTP outcalls.
#[test]
fn get_exchange_rate_will_charge_rate_limit_fee() {
    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(btreemap! {
            "BTC".to_string() => Ok(btc_queried_exchange_rate_with_failed_exchanges_mock(vec![])),
            "ICP".to_string() => Ok(icp_queried_exchange_rate_with_failed_exchanges_mock(vec![]))
        })
        .build();
    let env = TestEnvironment::builder()
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_MINIMUM_FEE_COST)
        .build();
    let request = GetExchangeRateRequest {
        base_asset: btc_asset(),
        quote_asset: icp_asset(),
        timestamp: Some(0),
    };

    set_request_counter(REQUEST_COUNTER_TRIGGER_RATE_LIMIT);
    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");
    assert!(matches!(result, Err(ExchangeRateError::RateLimited)));
}

/// This function tests to ensure a rate is returned when asking for a
/// crypto/USD pair.
#[test]
fn get_exchange_rate_for_crypto_usd_pair() {
    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(btreemap! {
            "ICP".to_string() => Ok(icp_queried_exchange_rate_with_failed_exchanges_mock(vec![]))
        })
        .with_get_stablecoin_rates_responses(btreemap! {
            USDS.to_string() => Ok(stablecoin_mock_with_failed_exchanges(USDS, &[RATE_UNIT], vec![])),
            USDC.to_string() => Ok(stablecoin_mock_with_failed_exchanges(USDC, &[RATE_UNIT], vec![])),
        })
        .build();
    let env = TestEnvironment::builder()
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_REQUEST_CYCLES_COST - XRC_IMMEDIATE_REFUND_CYCLES)
        .build();

    let request = GetExchangeRateRequest {
        base_asset: icp_asset(),
        quote_asset: usd_asset(),
        timestamp: Some(0),
    };
    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");
    assert!(
        matches!(result, Ok(ref rate) if rate.rate == 4 * RATE_UNIT),
        "Received the following result: {:#?}",
        result
    );
    assert_eq!(
        call_exchanges_impl
            .get_cryptocurrency_usdt_rate_calls
            .read()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        call_exchanges_impl
            .get_stablecoin_rates_calls
            .read()
            .unwrap()
            .len(),
        1
    );
}

/// This function tests to ensure a rate is returned when asking for a
/// USD/crypto pair.
#[test]
fn get_exchange_rate_for_usd_crypto_pair() {
    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(btreemap! {
            "ICP".to_string() => Ok(icp_queried_exchange_rate_with_failed_exchanges_mock(vec![]))
        })
        .with_get_stablecoin_rates_responses(btreemap! {
            USDS.to_string() => Ok(stablecoin_mock_with_failed_exchanges(USDS, &[RATE_UNIT], vec![])),
            USDC.to_string() => Ok(stablecoin_mock_with_failed_exchanges(USDC, &[RATE_UNIT], vec![]))
        })
        .build();
    let env = TestEnvironment::builder()
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_REQUEST_CYCLES_COST - XRC_IMMEDIATE_REFUND_CYCLES)
        .build();

    let request = GetExchangeRateRequest {
        base_asset: usd_asset(),
        quote_asset: icp_asset(),
        timestamp: Some(0),
    };

    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");
    assert!(
        matches!(result, Ok(ref rate) if rate.rate == 250_000_000),
        "Received the following result: {:#?}",
        result
    );
    assert_eq!(
        call_exchanges_impl
            .get_cryptocurrency_usdt_rate_calls
            .read()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        call_exchanges_impl
            .get_stablecoin_rates_calls
            .read()
            .unwrap()
            .len(),
        1
    );
}

/// This function tests to ensure a rate is returned when asking for a
/// crypto/non-USD pair.
#[test]
fn get_exchange_rate_for_crypto_non_usd_pair() {
    with_forex_rate_store_mut(|store| {
        store.put(
            0,
            btreemap! {
                    "EUR".to_string() =>
                        QueriedExchangeRate::new(
                            eur_asset(),
                            usd_asset(),
                            0,
                            &[800_000_000, 800_000_000, 800_000_000, 800_000_000],
                            4,
                            4,
                            Some(0),
                        ),
                    // It is necessary to have a CXDR rate with at least 4 sources for the rate store to return a result
                    COMPUTED_XDR_SYMBOL.to_string() => test_cxdr_rate(),
            },
        );
    });

    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(btreemap! {
            "ICP".to_string() => Ok(icp_queried_exchange_rate_with_failed_exchanges_mock(vec![]))
        })
        .with_get_stablecoin_rates_responses(btreemap! {
           USDS.to_string() => Ok(stablecoin_mock_with_failed_exchanges(USDS, &[RATE_UNIT], vec![])),
            USDC.to_string() => Ok(stablecoin_mock_with_failed_exchanges(USDC, &[RATE_UNIT], vec![]))
        })
        .build();
    let env = TestEnvironment::builder()
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_REQUEST_CYCLES_COST - XRC_IMMEDIATE_REFUND_CYCLES)
        .build();

    let request = GetExchangeRateRequest {
        base_asset: icp_asset(),
        quote_asset: eur_asset(),
        timestamp: Some(0),
    };
    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");
    assert!(
        matches!(result, Ok(ref rate) if rate.rate == 5 * RATE_UNIT),
        "Received the following result: {:#?}",
        result
    );
    assert_eq!(
        call_exchanges_impl
            .get_cryptocurrency_usdt_rate_calls
            .read()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        call_exchanges_impl
            .get_stablecoin_rates_calls
            .read()
            .unwrap()
            .len(),
        1
    );
}

/// This function tests to ensure a rate is returned when asking for a
/// non-USD/crypto pair.
#[test]
fn get_exchange_rate_for_non_usd_crypto_pair() {
    with_forex_rate_store_mut(|store| {
        store.put(
            0,
            btreemap! {
                    "EUR".to_string() =>
                        QueriedExchangeRate::new(
                            eur_asset(),
                            usd_asset(),
                            0,
                            &[800_000_000, 800_000_000, 800_000_000, 800_000_000],
                            4,
                            4,
                            Some(0),
                        ),
                    // It is necessary to have a CXDR rate with at least 4 sources for the rate store to return a result
                    COMPUTED_XDR_SYMBOL.to_string() => test_cxdr_rate(),
            },
        );
    });

    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(btreemap! {
            "ICP".to_string() => Ok(icp_queried_exchange_rate_with_failed_exchanges_mock(vec![]))
        })
        .with_get_stablecoin_rates_responses(btreemap! {
           USDS.to_string() => Ok(stablecoin_mock_with_failed_exchanges(USDS, &[RATE_UNIT], vec![])),
            USDC.to_string() => Ok(stablecoin_mock_with_failed_exchanges(USDC, &[RATE_UNIT], vec![]))
        })
        .build();
    let env = TestEnvironment::builder()
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_REQUEST_CYCLES_COST - XRC_IMMEDIATE_REFUND_CYCLES)
        .build();

    let request = GetExchangeRateRequest {
        base_asset: eur_asset(),
        quote_asset: icp_asset(),
        timestamp: Some(0),
    };
    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");
    assert!(
        matches!(result, Ok(ref rate) if rate.rate == 200_000_000),
        "Received the following result: {:#?}",
        result
    );
    assert_eq!(
        call_exchanges_impl
            .get_cryptocurrency_usdt_rate_calls
            .read()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        call_exchanges_impl
            .get_stablecoin_rates_calls
            .read()
            .unwrap()
            .len(),
        1
    );
}

/// This function tests to ensure an error CryptoQuoteAssetNotFound is returned when asking for a
/// non-USD/crypto pair and the crypto asset could be found.
#[test]
fn get_exchange_rate_for_non_usd_crypto_pair_crypto_asset_not_found() {
    with_forex_rate_store_mut(|store| {
        store.put(
            0,
            btreemap! {
                    "EUR".to_string() =>
                        QueriedExchangeRate::new(
                            eur_asset(),
                            usd_asset(),
                            0,
                            &[800_000_000, 800_000_000, 800_000_000, 800_000_000],
                            4,
                            4,
                            Some(0),
                        ),
                    // It is necessary to have a CXDR rate with at least 4 sources for the rate store to return a result
                    COMPUTED_XDR_SYMBOL.to_string() => test_cxdr_rate(),
            },
        );
    });

    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_stablecoin_rates_responses(btreemap! {
           USDS.to_string() => Ok(stablecoin_mock_with_failed_exchanges(USDS, &[RATE_UNIT], vec![])),
            USDC.to_string() => Ok(stablecoin_mock_with_failed_exchanges(USDC, &[RATE_UNIT], vec![]))
        })
        .build();
    let env = TestEnvironment::builder()
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_REQUEST_CYCLES_COST - XRC_IMMEDIATE_REFUND_CYCLES)
        .build();

    let request = GetExchangeRateRequest {
        base_asset: eur_asset(),
        quote_asset: icp_asset(),
        timestamp: Some(0),
    };
    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");
    assert!(
        matches!(result, Err(ExchangeRateError::CryptoQuoteAssetNotFound)),
        "Received the following result: {:#?}",
        result
    );
}

/// This function tests that an invalid timestamp error is returned when looking
/// up a rate when the fiat store does not contain a rate at a provided timestamp.
#[test]
fn get_crypto_fiat_pair_fails_when_the_fiat_timestamp_is_not_known() {
    let call_exchanges_impl = TestCallExchangesImpl::builder().build();
    let env = TestEnvironment::builder()
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_MINIMUM_FEE_COST)
        .build();

    let request = GetExchangeRateRequest {
        base_asset: icp_asset(),
        quote_asset: eur_asset(),
        timestamp: Some(0),
    };
    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");
    assert!(
        matches!(result, Err(ExchangeRateError::ForexInvalidTimestamp)),
        "Received the following result: {:#?}",
        result
    );
}

/// This function tests to ensure a rate is returned when asking for a
/// fiat pair.
#[test]
fn get_exchange_rate_for_fiat_eur_usd_pair() {
    let call_exchanges_impl = TestCallExchangesImpl::builder().build();
    let env = TestEnvironment::builder()
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_BASE_CYCLES_COST)
        .build();
    with_forex_rate_store_mut(|store| {
        store.put(
            0,
            btreemap! {
                    "EUR".to_string() =>
                        QueriedExchangeRate::new(
                            eur_asset(),
                            usd_asset(),
                            0,
                            &[800_000_000, 800_000_000, 800_000_000, 800_000_000],
                            4,
                            4,
                            Some(0),
                        ),
                    // It is necessary to have a CXDR rate with at least 4 sources for the rate store to return a result
                    COMPUTED_XDR_SYMBOL.to_string() => test_cxdr_rate(),
            },
        );
    });

    let request = GetExchangeRateRequest {
        base_asset: eur_asset(),
        quote_asset: usd_asset(),
        timestamp: Some(0),
    };
    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");
    assert!(
        matches!(result, Ok(ref rate) if rate.rate == 800_000_000),
        "Received the following result: {:#?}",
        result
    );
}

/// This function tests to ensure the minimum fee cost is accepted and an error is returned when
/// a known timestamp but unknown asset symbol is provided.
#[test]
fn get_exchange_rate_for_fiat_with_unknown_symbol() {
    let call_exchanges_impl = TestCallExchangesImpl::builder().build();
    let env = TestEnvironment::builder()
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_MINIMUM_FEE_COST)
        .build();
    with_forex_rate_store_mut(|store| {
        store.put(
            0,
            btreemap! {
                    "EUR".to_string() =>
                        QueriedExchangeRate::new(
                            eur_asset(),
                            usd_asset(),
                            0,
                            &[800_000_000, 800_000_000, 800_000_000, 800_000_000],
                            4,
                            4,
                            Some(0),
                        ),
                    // It is necessary to have a CXDR rate with at least 4 sources for the rate store to return a result
                    COMPUTED_XDR_SYMBOL.to_string() => test_cxdr_rate(),
            },
        );
    });

    let request = GetExchangeRateRequest {
        base_asset: Asset {
            symbol: "RTY".to_string(),
            class: AssetClass::FiatCurrency,
        },
        quote_asset: usd_asset(),
        timestamp: Some(0),
    };
    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");
    assert!(
        matches!(result, Err(ExchangeRateError::ForexBaseAssetNotFound)),
        "Received the following result: {:#?}",
        result
    );
}

/// This function tests to ensure the minimum fee cost is accepted and an error is returned when
/// a timestamp is not known to the forex store.
#[test]
fn get_exchange_rate_for_fiat_with_unknown_timestamp() {
    let call_exchanges_impl = TestCallExchangesImpl::builder().build();
    let env = TestEnvironment::builder()
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_MINIMUM_FEE_COST)
        .build();
    with_forex_rate_store_mut(|store| {
        store.put(
            86_400,
            btreemap! {
                    "EUR".to_string() =>
                        QueriedExchangeRate::new(
                            eur_asset(),
                            usd_asset(),
                            86_400,
                            &[800_000_000, 800_000_000, 800_000_000, 800_000_000],
                            4,
                            4,
                            Some(0),
                        ),
                    // It is necessary to have a CXDR rate with at least 4 sources for the rate store to return a result
                    COMPUTED_XDR_SYMBOL.to_string() => test_cxdr_rate(),
            },
        );
    });

    let request = GetExchangeRateRequest {
        base_asset: eur_asset(),
        quote_asset: usd_asset(),
        timestamp: Some(0),
    };
    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");
    assert!(
        matches!(result, Err(ExchangeRateError::ForexInvalidTimestamp)),
        "Received the following result: {:#?}",
        result
    );
}

/// This function tests that [get_exchange_rate] charges the minimum fee for usage when the request
/// is determined to be pending.
#[test]
fn get_exchange_rate_will_charge_minimum_fee_if_request_is_pending() {
    set_inflight_tracking(vec!["BTC".to_string(), "ICP".to_string()], 0);
    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(btreemap! {
          "BTC".to_string() => Ok(btc_queried_exchange_rate_with_failed_exchanges_mock(vec![])),
            "ICP".to_string() => Ok(icp_queried_exchange_rate_with_failed_exchanges_mock(vec![]))
        })
        .build();
    let env = TestEnvironment::builder()
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_MINIMUM_FEE_COST)
        .build();
    let request = GetExchangeRateRequest {
        base_asset: btc_asset(),
        quote_asset: icp_asset(),
        timestamp: Some(0),
    };

    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");
    assert!(matches!(result, Err(ExchangeRateError::Pending)));
}

/// This function tests that [get_exchange_rate] charges the maximum fee for usage when the request
/// contains symbol-timestamp pairs that are not currently inflight.
#[test]
fn get_exchange_rate_will_retrieve_rates_if_inflight_tracking_does_not_contain_symbol_timestamp_pairs(
) {
    set_inflight_tracking(vec!["AVAX".to_string(), "ICP".to_string()], 100);
    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(btreemap! {
          "BTC".to_string() => Ok(btc_queried_exchange_rate_with_failed_exchanges_mock(vec![])),
            "ICP".to_string() => Ok(icp_queried_exchange_rate_with_failed_exchanges_mock(vec![]))
        })
        .build();
    let env = TestEnvironment::builder()
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_BASE_CYCLES_COST + 2 * XRC_OUTBOUND_HTTP_CALL_CYCLES_COST)
        .build();
    let request = GetExchangeRateRequest {
        base_asset: btc_asset(),
        quote_asset: icp_asset(),
        timestamp: Some(0),
    };

    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");
    assert!(result.is_ok());
}

/// This function tests that [get_exchange_rate] charges the maximum fee for usage when the request
/// contains ANY symbol-timestamp pairs that are not currently inflight.
#[test]
fn get_exchange_rate_will_retrieve_rates_if_inflight_tracking_contains_any_symbol_timestamp_pairs()
{
    set_inflight_tracking(vec!["AVAX".to_string(), "ICP".to_string()], 0);
    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(btreemap! {
          "BTC".to_string() => Ok(btc_queried_exchange_rate_with_failed_exchanges_mock(vec![])),
            "ICP".to_string() => Ok(icp_queried_exchange_rate_with_failed_exchanges_mock(vec![]))
        })
        .build();
    let env = TestEnvironment::builder()
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_MINIMUM_FEE_COST)
        .build();
    let request = GetExchangeRateRequest {
        base_asset: btc_asset(),
        quote_asset: icp_asset(),
        timestamp: Some(0),
    };

    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");
    assert!(matches!(result, Err(ExchangeRateError::Pending)));
}

/// This function tests that [get_exchange_rate] can retrieve crypto/USDT rates with one set of outbound
/// calls.
#[test]
fn get_exchange_rate_can_retrieve_icp_usdt() {
    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(btreemap! {
            "ICP".to_string() => Ok(icp_queried_exchange_rate_with_failed_exchanges_mock(vec![]))
        })
        .build();
    let env = TestEnvironment::builder()
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_BASE_CYCLES_COST + XRC_OUTBOUND_HTTP_CALL_CYCLES_COST)
        .build();
    let request = GetExchangeRateRequest {
        base_asset: icp_asset(),
        quote_asset: usdt_asset(),
        timestamp: Some(0),
    };

    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");
    assert!(matches!(result, Ok(rate) if rate.base_asset.symbol == "ICP"));
    assert_eq!(
        call_exchanges_impl
            .get_cryptocurrency_usdt_rate_calls
            .read()
            .unwrap()
            .len(),
        1
    );
}

/// This function tests that [get_exchange_rate] can retrieve USDT/crypto rates with one set of outbound
/// calls.
#[test]
fn get_exchange_rate_can_retrieve_usdt_icp() {
    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(btreemap! {
            "ICP".to_string() => Ok(icp_queried_exchange_rate_with_failed_exchanges_mock(vec![]))
        })
        .build();
    let env = TestEnvironment::builder()
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_BASE_CYCLES_COST + XRC_OUTBOUND_HTTP_CALL_CYCLES_COST)
        .build();
    let request = GetExchangeRateRequest {
        base_asset: usdt_asset(),
        quote_asset: icp_asset(),
        timestamp: Some(0),
    };

    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");
    assert!(matches!(result, Ok(rate) if rate.quote_asset.symbol == "ICP"));
    assert_eq!(
        call_exchanges_impl
            .get_cryptocurrency_usdt_rate_calls
            .read()
            .unwrap()
            .len(),
        1
    );
}

mod privileged_asset_rate_limiting {
    use super::*;

    /// Helper to set up forex store with GBP for timestamp 0.
    fn setup_forex_store_gbp_at_0() {
        with_forex_rate_store_mut(|store| {
            store.put(
                0,
                btreemap! {
                    "GBP".to_string() =>
                        QueriedExchangeRate::new(
                            gbp_asset(),
                            usd_asset(),
                            0,
                            &[1_350_000_000, 1_350_000_000, 1_350_000_000, 1_350_000_000],
                            4,
                            4,
                            Some(0),
                        ),
                    COMPUTED_XDR_SYMBOL.to_string() => test_cxdr_rate(),
                },
            );
        });
    }

    fn call_exchanges_impl() -> TestCallExchangesImpl {
        TestCallExchangesImpl::builder()
            .with_get_cryptocurrency_usdt_rate_responses(btreemap! {
                "BTC".to_string() => Ok(btc_queried_exchange_rate_with_failed_exchanges_mock(vec![])),
                "PEPE".to_string() => Err(CallExchangeError::NoRatesFound),
                "ICP".to_string() => Ok(icp_queried_exchange_rate_with_failed_exchanges_mock(vec![]))
            })
            .with_get_stablecoin_rates_responses(btreemap! {
                USDS.to_string() => Ok(stablecoin_mock_with_failed_exchanges(USDS, &[RATE_UNIT], vec![])),
                USDC.to_string() => Ok(stablecoin_mock_with_failed_exchanges(USDC, &[RATE_UNIT], vec![])),
            })
            .build()
    }

    /// Privileged pair (BTC-GBP) with timestamp None: rate limiter is bypassed.
    #[test]
    fn privileged_pair_timestamp_none_bypasses_rate_limiter() {
        setup_forex_store_gbp_at_0();
        let current_timestamp: u64 = 100;
        let env = TestEnvironment::builder()
            .with_time_secs(current_timestamp)
            .with_cycles_available(XRC_REQUEST_CYCLES_COST)
            .with_accepted_cycles(XRC_REQUEST_CYCLES_COST - XRC_IMMEDIATE_REFUND_CYCLES)
            .build();
        let request = GetExchangeRateRequest {
            base_asset: btc_asset(),
            quote_asset: gbp_asset(),
            timestamp: None,
        };
        set_request_counter(REQUEST_COUNTER_TRIGGER_RATE_LIMIT);

        let result = get_exchange_rate_internal(&env, &call_exchanges_impl(), &request)
            .now_or_never()
            .expect("future should complete");

        assert!(
            result.is_ok(),
            "Privileged pair with timestamp None should bypass rate limiter, got: {:#?}",
            result
        );
    }

    /// Privileged pair (BTC-GBP) with timestamp Some(current): rate limiter is bypassed.
    #[test]
    fn privileged_pair_timestamp_current_hits_rate_limiter() {
        setup_forex_store_gbp_at_0();
        let current_timestamp: u64 = 100;
        let env = TestEnvironment::builder()
            .with_time_secs(current_timestamp)
            .with_cycles_available(XRC_REQUEST_CYCLES_COST)
            .with_accepted_cycles(XRC_MINIMUM_FEE_COST)
            .build();
        let request = GetExchangeRateRequest {
            base_asset: btc_asset(),
            quote_asset: gbp_asset(),
            timestamp: Some(current_timestamp),
        };
        set_request_counter(REQUEST_COUNTER_TRIGGER_RATE_LIMIT);

        let result = get_exchange_rate_internal(&env, &call_exchanges_impl(), &request)
            .now_or_never()
            .expect("future should complete");

        assert!(
            matches!(result, Err(ExchangeRateError::RateLimited)),
            "Privileged pair with current timestamp set should be rate limited, got: {:#?}",
            result
        );
    }

    /// Privileged pair (BTC-GBP) with timestamp in the past: rate limiter applies (no bypass).
    #[test]
    fn privileged_pair_timestamp_past_hits_rate_limiter() {
        setup_forex_store_gbp_at_0();
        let current_timestamp: u64 = 150; // so that requested 0 is not "recent"
        let env = TestEnvironment::builder()
            .with_time_secs(current_timestamp)
            .with_cycles_available(XRC_REQUEST_CYCLES_COST)
            .with_accepted_cycles(XRC_MINIMUM_FEE_COST)
            .build();
        let request = GetExchangeRateRequest {
            base_asset: btc_asset(),
            quote_asset: gbp_asset(),
            timestamp: Some(0),
        };
        set_request_counter(REQUEST_COUNTER_TRIGGER_RATE_LIMIT);

        let result = get_exchange_rate_internal(&env, &call_exchanges_impl(), &request)
            .now_or_never()
            .expect("future should complete");

        assert!(
            matches!(result, Err(ExchangeRateError::RateLimited)),
            "Privileged pair with timestamp in the past should be rate limited, got: {:#?}",
            result
        );
    }

    /// This function tests that [get_exchange_rate] returns [ExchangeRateError::RateLimited]
    /// for a non-privileged crypto-fiat pair (PEPE-GBP) when the rate limit is hit.
    /// Uses a request timestamp in the past (0) and current time 150 so the timestamp is not "recent".
    #[test]
    fn unprivileged_pair_hits_rate_limiter() {
        setup_forex_store_gbp_at_0();
        let env = TestEnvironment::builder()
            .with_cycles_available(XRC_REQUEST_CYCLES_COST)
            .with_accepted_cycles(XRC_MINIMUM_FEE_COST)
            .build();
        let request = GetExchangeRateRequest {
            base_asset: pepe_asset(),
            quote_asset: gbp_asset(),
            timestamp: None,
        };
        set_request_counter(REQUEST_COUNTER_TRIGGER_RATE_LIMIT);

        let result = get_exchange_rate_internal(&env, &call_exchanges_impl(), &request)
            .now_or_never()
            .expect("future should complete");

        assert!(
            matches!(result, Err(ExchangeRateError::RateLimited)),
            "Expected RateLimited for PEPE-GBP when rate limit is hit, got: {:#?}",
            result
        );
    }

    /// This function tests that [get_exchange_rate] allows privileged callers to bypass the pending check (crytpo pair).
    #[test]
    fn get_exchange_rate_will_allow_a_privileged_caller_to_bypass_pending_check_crypto_pair() {
        set_inflight_tracking(vec!["BTC".to_string(), "ICP".to_string()], 0);
        let env = TestEnvironment::builder()
            .with_cycles_available(XRC_REQUEST_CYCLES_COST)
            .with_accepted_cycles(XRC_MINIMUM_FEE_COST)
            .with_caller(PRIVILEGED_CANISTER_IDS[0])
            .build();
        let request = GetExchangeRateRequest {
            base_asset: btc_asset(),
            quote_asset: icp_asset(),
            timestamp: Some(0),
        };

        let result = get_exchange_rate_internal(&env, &call_exchanges_impl(), &request)
            .now_or_never()
            .expect("future should complete");

        assert!(result.is_ok());
    }

    /// This function tests that [get_exchange_rate] allows privileged callers to bypass the pending check (crypto-fiat pair).
    #[test]
    fn get_exchange_rate_will_allow_a_privileged_caller_to_bypass_pending_check_crypto_fiat_pair() {
        set_inflight_tracking(vec!["BTC".to_string(), "ICP".to_string()], 0);
        let env = TestEnvironment::builder()
            .with_cycles_available(XRC_REQUEST_CYCLES_COST)
            .with_accepted_cycles(XRC_MINIMUM_FEE_COST)
            .with_caller(PRIVILEGED_CANISTER_IDS[0])
            .build();
        let request = GetExchangeRateRequest {
            base_asset: icp_asset(),
            quote_asset: usd_asset(),
            timestamp: Some(0),
        };

        let result = get_exchange_rate_internal(&env, &call_exchanges_impl(), &request)
            .now_or_never()
            .expect("future should complete");

        assert!(result.is_ok());
    }
}

mod uses_previous_minute_when_timestamp_is_null_if_request_would_be_pending {
    use super::*;

    /// This function tests that [get_exchange_rate] will return a rate for a crypto pair when:
    /// * timestamp is null
    /// * the cryto pair with the previous minute IS in the cache
    #[test]
    fn crypto_pair_when_cache_contains_the_rates() {
        with_cache_mut(|cache| {
            cache.insert(&icp_queried_exchange_rate_mock());
            cache.insert(&btc_queried_exchange_rate_mock());
        });
        set_inflight_tracking(vec!["BTC".to_string(), "ICP".to_string()], 60);
        let call_exchanges_impl = TestCallExchangesImpl::builder()
            .with_get_cryptocurrency_usdt_rate_responses(btreemap! {
                "BTC".to_string() => Ok(btc_queried_exchange_rate_with_failed_exchanges_mock(vec![])),
                "ICP".to_string() => Ok(icp_queried_exchange_rate_with_failed_exchanges_mock(vec![]))
            })
            .build();
        let env = TestEnvironment::builder()
            .with_cycles_available(XRC_REQUEST_CYCLES_COST)
            .with_accepted_cycles(XRC_BASE_CYCLES_COST)
            .with_time_secs(90)
            .build();
        let request = GetExchangeRateRequest {
            base_asset: btc_asset(),
            quote_asset: icp_asset(),
            timestamp: None,
        };

        let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
            .now_or_never()
            .expect("future should complete");

        assert!(matches!(result, Ok(rate) if rate.timestamp == 0));
    }

    /// This function tests that [get_exchange_rate] will return pending for a crypto pair when:
    /// * timestamp is null
    /// * the crypto pair with the previous minute is not in the cache
    #[test]
    fn crypto_pair_when_the_cache_does_not_contain_the_rates() {
        set_inflight_tracking(vec!["BTC".to_string(), "ICP".to_string()], 60);
        let call_exchanges_impl = TestCallExchangesImpl::builder()
            .with_get_cryptocurrency_usdt_rate_responses(btreemap! {
                "BTC".to_string() => Ok(btc_queried_exchange_rate_with_failed_exchanges_mock(vec![])),
                "ICP".to_string() => Ok(icp_queried_exchange_rate_with_failed_exchanges_mock(vec![]))
            })
            .build();
        let env = TestEnvironment::builder()
            .with_cycles_available(XRC_REQUEST_CYCLES_COST)
            .with_accepted_cycles(XRC_MINIMUM_FEE_COST)
            .with_time_secs(90)
            .build();
        let request = GetExchangeRateRequest {
            base_asset: btc_asset(),
            quote_asset: icp_asset(),
            timestamp: None,
        };

        let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
            .now_or_never()
            .expect("future should complete");
        assert!(matches!(result, Err(ExchangeRateError::Pending)));
    }

    /// This function tests that [get_exchange_rate] will return a rate for a crypto/fiat pair when:
    /// * timestamp is null
    /// * there is a pending lookup for the crypto asset for the current minute
    /// * the crypto asset with the previous minute IS in the cache
    /// * the stablecoins are in the cache
    #[test]
    fn crypto_fiat_pair_has_asset_and_stablecoins_in_cache() {
        with_cache_mut(|cache| {
            cache.insert(&icp_queried_exchange_rate_mock());
            cache.insert(&stablecoin_mock(USDS, &[RATE_UNIT]));
            cache.insert(&stablecoin_mock(USDC, &[RATE_UNIT]));
        });
        set_inflight_tracking(vec!["BTC".to_string(), "ICP".to_string()], 60);
        let call_exchanges_impl = TestCallExchangesImpl::builder().build();
        let env = TestEnvironment::builder()
            .with_cycles_available(XRC_REQUEST_CYCLES_COST)
            .with_accepted_cycles(XRC_BASE_CYCLES_COST)
            .with_time_secs(90)
            .build();
        let request = GetExchangeRateRequest {
            base_asset: icp_asset(),
            quote_asset: usd_asset(),
            timestamp: None,
        };

        let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
            .now_or_never()
            .expect("future should complete");
        assert!(matches!(result, Ok(rate) if rate.timestamp == 0));
    }

    /// This function tests that [get_exchange_rate] will return pending for a crypto/fiat pair when:
    /// * timestamp is null
    /// * the crypto asset with the previous minute IS in the cache
    /// * the stablecoins are NOT in the cache
    #[test]
    fn crypto_fiat_pair_does_not_have_stablecoins_in_cache() {
        with_cache_mut(|cache| {
            cache.insert(&icp_queried_exchange_rate_mock());
        });
        set_inflight_tracking(vec!["BTC".to_string(), "ICP".to_string()], 60);
        let call_exchanges_impl = TestCallExchangesImpl::builder().build();
        let env = TestEnvironment::builder()
            .with_cycles_available(XRC_REQUEST_CYCLES_COST)
            .with_accepted_cycles(XRC_MINIMUM_FEE_COST)
            .with_time_secs(90)
            .build();
        let request = GetExchangeRateRequest {
            base_asset: icp_asset(),
            quote_asset: usd_asset(),
            timestamp: None,
        };

        let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
            .now_or_never()
            .expect("future should complete");
        assert!(matches!(result, Err(ExchangeRateError::Pending)));
    }

    /// This function tests that [get_exchange_rate] will return pending for a crypto/fiat pair when:
    /// * timestamp is null
    /// * the crypto asset with the previous minute IS NOT in the cache
    /// * the stablecoins are in the cache
    #[test]
    fn crypto_fiat_pair_does_not_have_crypto_asset_in_cache() {
        with_cache_mut(|cache| {
            cache.insert(&stablecoin_mock(USDS, &[RATE_UNIT]));
            cache.insert(&stablecoin_mock(USDC, &[RATE_UNIT]));
        });
        set_inflight_tracking(vec!["BTC".to_string(), "ICP".to_string()], 60);
        let call_exchanges_impl = TestCallExchangesImpl::builder().build();
        let env = TestEnvironment::builder()
            .with_cycles_available(XRC_REQUEST_CYCLES_COST)
            .with_accepted_cycles(XRC_MINIMUM_FEE_COST)
            .with_time_secs(90)
            .build();
        let request = GetExchangeRateRequest {
            base_asset: icp_asset(),
            quote_asset: usd_asset(),
            timestamp: None,
        };

        let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
            .now_or_never()
            .expect("future should complete");
        assert!(matches!(result, Err(ExchangeRateError::Pending)));
    }
}

/// This function tests to ensure a rate is returned when asking for a
/// USD/crypto pair with lowercase symbols.
#[test]
fn get_exchange_rate_with_unsanitized_request_to_ensure_requests_are_sanitized() {
    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(btreemap! {
            "ICP".to_string() => Ok(icp_queried_exchange_rate_with_failed_exchanges_mock(vec![]))
        })
        .with_get_stablecoin_rates_responses(btreemap! {
           USDS.to_string() => Ok(stablecoin_mock_with_failed_exchanges(USDS, &[RATE_UNIT], vec![])),
            USDC.to_string() => Ok(stablecoin_mock_with_failed_exchanges(USDC, &[RATE_UNIT], vec![]))
        })
        .build();
    let env = TestEnvironment::builder()
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_REQUEST_CYCLES_COST - XRC_IMMEDIATE_REFUND_CYCLES)
        .build();

    let request = GetExchangeRateRequest {
        quote_asset: icp_asset(),
        base_asset: usd_asset(),
        timestamp: Some(0),
    };

    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");
    assert!(
        matches!(result, Ok(ref rate) if rate.rate == 250_000_000),
        "Received the following result: {:#?}",
        result
    );
    assert_eq!(
        call_exchanges_impl
            .get_cryptocurrency_usdt_rate_calls
            .read()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        call_exchanges_impl
            .get_stablecoin_rates_calls
            .read()
            .unwrap()
            .len(),
        1
    );
}

/// This test ensures that privileged canisters only get cached exchange rates if there are at least
/// [MIN_MIN_NUM_RATES_FOR_PRIVILEGED_CANISTERS] collected rates.
#[test]
fn cached_rate_with_few_collected_rates_is_ignored_for_privileged_canister() {
    // The cached ICP/USDT rate is 8*RATE_UNIT.
    with_cache_mut(|cache| {
        cache.insert(&icp_queried_exchange_rate_with_one_rate_mock());
    });

    // The exchanges return an ICP/USDT rate of 4*RATE_UNIT.
    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(btreemap! {
            "ICP".to_string() => Ok(icp_queried_exchange_rate_with_failed_exchanges_mock(vec![]))
        })
        .build();

    let env = TestEnvironment::builder()
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_BASE_CYCLES_COST)
        .build();
    let request = GetExchangeRateRequest {
        base_asset: icp_asset(),
        quote_asset: usdt_asset(),
        timestamp: None,
    };

    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");
    // The cached rate should be returned.
    assert!(matches!(result, Ok(rate) if rate.rate == 8000000000));

    let env = TestEnvironment::builder()
        .with_cycles_available(0)
        .with_caller(PRIVILEGED_CANISTER_IDS[0])
        .build();

    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");
    // The rate received from exchanges should be returned.
    assert!(matches!(result, Ok(rate) if rate.rate == 4000000000));
}

mod timestamp_is_in_future {

    use crate::{errors::TIMESTAMP_IS_IN_FUTURE_ERROR_CODE, ONE_MINUTE_SECONDS};
    use ic_xrc_types::OtherError;

    use super::*;

    /// This function tests that a crypto pair request with a timestamp in the future
    /// is rejected and charged the minimum fee.
    #[test]
    fn handle_cryptocurrency_pair() {
        let current_timestamp: u64 = 1678752000;
        let future_timestamp = current_timestamp.saturating_add(ONE_MINUTE_SECONDS);
        let call_exchanges_impl = TestCallExchangesImpl::builder().build();
        let env = TestEnvironment::builder()
            .with_time_secs(current_timestamp)
            .with_cycles_available(XRC_REQUEST_CYCLES_COST)
            .with_accepted_cycles(XRC_MINIMUM_FEE_COST)
            .build();
        let request = GetExchangeRateRequest {
            base_asset: btc_asset(),
            quote_asset: icp_asset(),
            timestamp: Some(future_timestamp),
        };

        let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
            .now_or_never()
            .expect("future should complete");
        assert!(
            matches!(result, Err(ExchangeRateError::Other(OtherError { code, description: _ })) if code == TIMESTAMP_IS_IN_FUTURE_ERROR_CODE)
        );
    }

    /// This function tests that a crypto/fiat pair request with a timestamp in the future
    /// is rejected and charged the minimum fee.
    #[test]
    fn handle_crypto_base_fiat_quote_pair() {
        let current_timestamp: u64 = 1678752000;
        let future_timestamp = current_timestamp.saturating_add(ONE_MINUTE_SECONDS);
        let call_exchanges_impl = TestCallExchangesImpl::builder().build();
        let env = TestEnvironment::builder()
            .with_time_secs(current_timestamp)
            .with_cycles_available(XRC_REQUEST_CYCLES_COST)
            .with_accepted_cycles(XRC_MINIMUM_FEE_COST)
            .build();
        let request = GetExchangeRateRequest {
            base_asset: icp_asset(),
            quote_asset: usd_asset(),
            timestamp: Some(future_timestamp),
        };

        let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
            .now_or_never()
            .expect("future should complete");
        assert!(
            matches!(result, Err(ExchangeRateError::Other(OtherError { code, description: _ })) if code == TIMESTAMP_IS_IN_FUTURE_ERROR_CODE)
        );
    }

    /// This function tests that a fiat pair request with a timestamp in the future
    /// is rejected and charged the minimum fee.
    #[test]
    fn handle_fiat_pair() {
        let current_timestamp: u64 = 1678752000;
        let future_timestamp = current_timestamp.saturating_add(ONE_MINUTE_SECONDS);
        let call_exchanges_impl = TestCallExchangesImpl::builder().build();
        let env = TestEnvironment::builder()
            .with_time_secs(current_timestamp)
            .with_cycles_available(XRC_REQUEST_CYCLES_COST)
            .with_accepted_cycles(XRC_MINIMUM_FEE_COST)
            .build();
        let request = GetExchangeRateRequest {
            base_asset: eur_asset(),
            quote_asset: usd_asset(),
            timestamp: Some(future_timestamp),
        };

        let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
            .now_or_never()
            .expect("future should complete");
        assert!(
            matches!(result, Err(ExchangeRateError::Other(OtherError { code, description: _ })) if code == TIMESTAMP_IS_IN_FUTURE_ERROR_CODE)
        );
    }

    /// This function tests that a privileged caller's request with a timestamp in the future
    /// is rejected and not charged.
    #[test]
    fn privileged_caller_cannot_request_a_timestamp_in_the_future() {
        let current_timestamp: u64 = 1678752000;
        let future_timestamp = current_timestamp.saturating_add(ONE_MINUTE_SECONDS);
        let call_exchanges_impl = TestCallExchangesImpl::builder().build();
        let env = TestEnvironment::builder()
            .with_caller(PRIVILEGED_CANISTER_IDS[0])
            .with_time_secs(current_timestamp)
            .build();
        let request = GetExchangeRateRequest {
            base_asset: btc_asset(),
            quote_asset: icp_asset(),
            timestamp: Some(future_timestamp),
        };

        let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
            .now_or_never()
            .expect("future should complete");
        assert!(
            matches!(result, Err(ExchangeRateError::Other(OtherError { code, description: _ })) if code == TIMESTAMP_IS_IN_FUTURE_ERROR_CODE)
        );
    }
}

mod request_contains_invalid_symbols {

    use ic_xrc_types::OtherError;

    use crate::errors;

    use super::*;

    /// This function tests that a crypto pair request with an invalid base asset symbol
    /// is rejected and charged the minimum fee.
    #[test]
    fn handle_cryptocurrency_pair_invalid_base_asset_symbol() {
        let current_timestamp: u64 = 1678752000;
        let call_exchanges_impl = TestCallExchangesImpl::builder().build();
        let env = TestEnvironment::builder()
            .with_time_secs(current_timestamp)
            .with_cycles_available(XRC_REQUEST_CYCLES_COST)
            .with_accepted_cycles(XRC_MINIMUM_FEE_COST)
            .build();
        let request = GetExchangeRateRequest {
            base_asset: Asset {
                symbol: "<>".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            quote_asset: icp_asset(),
            timestamp: Some(current_timestamp),
        };

        let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
            .now_or_never()
            .expect("future should complete");
        assert!(matches!(
            result,
            Err(ExchangeRateError::Other(OtherError { code, description })) if code == errors::BASE_ASSET_INVALID_SYMBOL_ERROR_CODE && description == errors::BASE_ASSET_INVALID_SYMBOL_ERROR_MESSAGE
        ));
    }

    /// This function tests that a crypto pair request with an invalid quote asset symbol
    /// is rejected and charged the minimum fee.
    #[test]
    fn handle_cryptocurrency_pair_invalid_quote_asset_symbol() {
        let current_timestamp: u64 = 1678752000;
        let call_exchanges_impl = TestCallExchangesImpl::builder().build();
        let env = TestEnvironment::builder()
            .with_time_secs(current_timestamp)
            .with_cycles_available(XRC_REQUEST_CYCLES_COST)
            .with_accepted_cycles(XRC_MINIMUM_FEE_COST)
            .build();
        let request = GetExchangeRateRequest {
            base_asset: icp_asset(),
            quote_asset: Asset {
                symbol: "/ç%^*@ßðæđßħłĸ¶ł«»¢nµþœŧ€đŋ".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            timestamp: Some(current_timestamp),
        };

        let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
            .now_or_never()
            .expect("future should complete");
        assert!(matches!(
            result,
            Err(ExchangeRateError::Other(OtherError { code, description })) if code == errors::QUOTE_ASSET_INVALID_SYMBOL_ERROR_CODE && description == errors::QUOTE_ASSET_INVALID_SYMBOL_ERROR_MESSAGE
        ));
    }

    /// This function tests that a crypto/fiat pair request with an invalid base asset symbol
    /// is rejected and charged the minimum fee.
    #[test]
    fn handle_crypto_base_fiat_quote_pair_invalid_base_asset() {
        let current_timestamp: u64 = 1678752000;
        let call_exchanges_impl = TestCallExchangesImpl::builder().build();
        let env = TestEnvironment::builder()
            .with_time_secs(current_timestamp)
            .with_cycles_available(XRC_REQUEST_CYCLES_COST)
            .with_accepted_cycles(XRC_MINIMUM_FEE_COST)
            .build();
        let request = GetExchangeRateRequest {
            base_asset: Asset {
                symbol: "-)]}:@[!]+.;!#_-&$,;{%$@&;=]?%".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            quote_asset: usd_asset(),
            timestamp: Some(current_timestamp),
        };

        let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
            .now_or_never()
            .expect("future should complete");
        assert!(matches!(
            result,
            Err(ExchangeRateError::Other(OtherError { code, description })) if code == errors::BASE_ASSET_INVALID_SYMBOL_ERROR_CODE && description == errors::BASE_ASSET_INVALID_SYMBOL_ERROR_MESSAGE
        ));
    }

    /// This function tests that a crypto/fiat pair request with an invalid quote asset symbol
    /// is rejected and charged the minimum fee.
    #[test]
    fn handle_crypto_base_fiat_quote_pair_invalid_quote_asset() {
        let current_timestamp: u64 = 1678752000;
        let call_exchanges_impl = TestCallExchangesImpl::builder().build();
        let env = TestEnvironment::builder()
            .with_time_secs(current_timestamp)
            .with_cycles_available(XRC_REQUEST_CYCLES_COST)
            .with_accepted_cycles(XRC_MINIMUM_FEE_COST)
            .build();
        let request = GetExchangeRateRequest {
            base_asset: icp_asset(),
            quote_asset: Asset {
                symbol: ";+#]=/)+%$.$@[?]/]}.-:#+!.-[]#".to_string(),
                class: AssetClass::FiatCurrency,
            },
            timestamp: Some(current_timestamp),
        };

        let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
            .now_or_never()
            .expect("future should complete");

        assert!(matches!(
            result,
            Err(ExchangeRateError::Other(OtherError { code, description })) if code == errors::QUOTE_ASSET_INVALID_SYMBOL_ERROR_CODE && description == errors::QUOTE_ASSET_INVALID_SYMBOL_ERROR_MESSAGE
        ));
    }

    /// This function tests that a crypto/fiat pair request with an invalid base asset symbol
    /// is rejected and charged the minimum fee.
    #[test]
    fn handle_fiat_base_cypto_quote_pair_invalid_base_asset() {
        let current_timestamp: u64 = 1678752000;
        let call_exchanges_impl = TestCallExchangesImpl::builder().build();
        let env = TestEnvironment::builder()
            .with_time_secs(current_timestamp)
            .with_cycles_available(XRC_REQUEST_CYCLES_COST)
            .with_accepted_cycles(XRC_MINIMUM_FEE_COST)
            .build();
        let request = GetExchangeRateRequest {
            base_asset: Asset {
                symbol: ":*(@;,[!])*?:@&]:;-*+-)(?,#?[:>".to_string(),
                class: AssetClass::FiatCurrency,
            },
            quote_asset: icp_asset(),
            timestamp: Some(current_timestamp),
        };

        let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
            .now_or_never()
            .expect("future should complete");
        assert!(matches!(
            result,
            Err(ExchangeRateError::Other(OtherError { code, description })) if code == errors::BASE_ASSET_INVALID_SYMBOL_ERROR_CODE && description == errors::BASE_ASSET_INVALID_SYMBOL_ERROR_MESSAGE
        ));
    }

    /// This function tests that a fiat/crypto pair request with an invalid quote asset symbol
    /// is rejected and charged the minimum fee.
    #[test]
    fn handle_fiat_base_crypto_quote_pair_invalid_quote_asset() {
        let current_timestamp: u64 = 1678752000;
        let call_exchanges_impl = TestCallExchangesImpl::builder().build();
        let env = TestEnvironment::builder()
            .with_time_secs(current_timestamp)
            .with_cycles_available(XRC_REQUEST_CYCLES_COST)
            .with_accepted_cycles(XRC_MINIMUM_FEE_COST)
            .build();
        let request = GetExchangeRateRequest {
            base_asset: usd_asset(),
            quote_asset: Asset {
                symbol: "@!!!@&%!$&#@*$&=$&=@".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            timestamp: Some(current_timestamp),
        };

        let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
            .now_or_never()
            .expect("future should complete");
        assert!(matches!(
            result,
            Err(ExchangeRateError::Other(OtherError { code, description })) if code == errors::QUOTE_ASSET_INVALID_SYMBOL_ERROR_CODE && description == errors::QUOTE_ASSET_INVALID_SYMBOL_ERROR_MESSAGE
        ));
    }

    /// This function tests that a fiat pair request with an invalid base asset symbol
    /// is rejected and charged the minimum fee.
    #[test]
    fn handle_fiat_pair_invalid_base_asset() {
        let current_timestamp: u64 = 1678752000;
        let call_exchanges_impl = TestCallExchangesImpl::builder().build();
        let env = TestEnvironment::builder()
            .with_time_secs(current_timestamp)
            .with_cycles_available(XRC_REQUEST_CYCLES_COST)
            .with_accepted_cycles(XRC_MINIMUM_FEE_COST)
            .build();
        let request = GetExchangeRateRequest {
            base_asset: Asset {
                symbol: "+!!*%$#%%&=&*$!%%=%#".to_string(),
                class: AssetClass::FiatCurrency,
            },
            quote_asset: usd_asset(),
            timestamp: Some(current_timestamp),
        };

        let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
            .now_or_never()
            .expect("future should complete");
        assert!(matches!(
            result,
            Err(ExchangeRateError::Other(OtherError { code, description })) if code == errors::BASE_ASSET_INVALID_SYMBOL_ERROR_CODE && description == errors::BASE_ASSET_INVALID_SYMBOL_ERROR_MESSAGE
        ));
    }

    /// This function tests that a fiat pair request with an invalid quote asset symbol
    /// is rejected and charged the minimum fee.
    #[test]
    fn handle_fiat_pair_invalid_quote_asset() {
        let current_timestamp: u64 = 1678752000;
        let call_exchanges_impl = TestCallExchangesImpl::builder().build();
        let env = TestEnvironment::builder()
            .with_time_secs(current_timestamp)
            .with_cycles_available(XRC_REQUEST_CYCLES_COST)
            .with_accepted_cycles(XRC_MINIMUM_FEE_COST)
            .build();
        let request = GetExchangeRateRequest {
            base_asset: usd_asset(),
            quote_asset: Asset {
                symbol: "<>".to_string(),
                class: AssetClass::FiatCurrency,
            },
            timestamp: Some(current_timestamp),
        };

        let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
            .now_or_never()
            .expect("future should complete");
        assert!(matches!(
            result,
            Err(ExchangeRateError::Other(OtherError { code, description })) if code == errors::QUOTE_ASSET_INVALID_SYMBOL_ERROR_CODE && description == errors::QUOTE_ASSET_INVALID_SYMBOL_ERROR_MESSAGE
        ));
    }

    /// This function tests that a privileged caller's request with an invalid asset symbol
    /// is rejected and not charged.
    #[test]
    fn privileged_caller_cannot_request_with_an_invalid_symbol() {
        let current_timestamp: u64 = 1678752000;
        let call_exchanges_impl = TestCallExchangesImpl::builder().build();
        let env = TestEnvironment::builder()
            .with_caller(PRIVILEGED_CANISTER_IDS[0])
            .with_time_secs(current_timestamp)
            .build();
        let request = GetExchangeRateRequest {
            base_asset: Asset {
                symbol: "⭥⁸⣩⁤₨␔⊁ ⋦ⵕ⬌⇧ⶢ".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            quote_asset: icp_asset(),
            timestamp: Some(current_timestamp),
        };

        let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
            .now_or_never()
            .expect("future should complete");
        assert!(matches!(
            result,
            Err(ExchangeRateError::Other(OtherError { code, description })) if code == errors::BASE_ASSET_INVALID_SYMBOL_ERROR_CODE && description == errors::BASE_ASSET_INVALID_SYMBOL_ERROR_MESSAGE
        ));
    }
}

mod stablecoin_symbol_metrics {
    use super::super::record_stablecoin_symbol_rates_received;
    use crate::{
        make_metric_key, reset_labeled_metrics_for_test, with_labeled_counters, LabelKey,
        MetricName,
    };

    fn reset() {
        reset_labeled_metrics_for_test();
    }

    fn usds_key() -> crate::MetricKey {
        make_metric_key(
            MetricName::StablecoinSymbolRatesReceived,
            &[(LabelKey::Symbol, "USDS")],
        )
    }

    #[test]
    fn counts_usable_rates_across_calls() {
        reset();
        record_stablecoin_symbol_rates_received("USDS", &[1, 2, 3, 4, 5, 6]);
        record_stablecoin_symbol_rates_received("USDS", &[10, 20]);

        with_labeled_counters(|m| {
            assert_eq!(m.get(&usds_key()).copied(), Some(8));
        });
    }

    #[test]
    fn excludes_zero_rates_from_the_count() {
        // The fix-point of this test: `Ok(0)` results reach
        // `get_stablecoin_rate` in the non-invert path (e.g. USDS/USDT
        // pairs where USDS is the base) and would otherwise inflate the
        // counter even though `QueriedExchangeRate::new` drops them
        // downstream. The filter must live inside the recorder so the
        // call site can't accidentally count raw `rates.len()`.
        reset();
        record_stablecoin_symbol_rates_received("USDS", &[0, 5_000, 0, 1_000, 0]);

        with_labeled_counters(|m| {
            assert_eq!(
                m.get(&usds_key()).copied(),
                Some(2),
                "only the two non-zero rates should count"
            );
        });
    }

    #[test]
    fn materialises_series_on_empty_input() {
        // The DAI-rebrand scenario: a symbol that produces no usable
        // rates at all. The series must exist after the first call so
        // a `rate(...) == 0` alert has something to evaluate against
        // from t=0, even before any non-zero rate has ever been seen.
        reset();
        record_stablecoin_symbol_rates_received("USDS", &[]);

        with_labeled_counters(|m| {
            assert_eq!(m.get(&usds_key()).copied(), Some(0));
        });
    }

    #[test]
    fn materialises_series_when_all_inputs_are_zero() {
        // Same materialisation guarantee, but with the filter actually
        // exercised — every exchange responded with `Ok(0)` so the
        // input slice is non-empty but the usable count is still zero.
        reset();
        record_stablecoin_symbol_rates_received("USDS", &[0, 0, 0]);

        with_labeled_counters(|m| {
            assert_eq!(m.get(&usds_key()).copied(), Some(0));
        });
    }

    #[test]
    fn separates_symbols() {
        reset();
        record_stablecoin_symbol_rates_received("USDS", &[1, 2, 3]);
        record_stablecoin_symbol_rates_received("USDC", &[1, 2, 3, 4, 5]);

        with_labeled_counters(|m| {
            assert_eq!(m.len(), 2);
            let usdc = make_metric_key(
                MetricName::StablecoinSymbolRatesReceived,
                &[(LabelKey::Symbol, "USDC")],
            );
            assert_eq!(m.get(&usds_key()).copied(), Some(3));
            assert_eq!(m.get(&usdc).copied(), Some(5));
        });
    }
}

/// The crypto path queries only exchanges whose discovered listing contains the
/// requested base: an exchange with a fresh listing that omits the base is
/// dropped, while exchanges with no listing fail open and are kept. This guards
/// the listing-based gating wired into `get_cryptocurrency_usdt_rate`.
#[test]
fn exchanges_listing_base_against_usdt_filters_by_listing() {
    let now_secs = 1_000;
    let exchanges: Vec<&Exchange> = EXCHANGES.iter().collect();
    let gated = exchanges[0];

    // Clean slate, then give one exchange a fresh listing that includes BTC but
    // not ICP. Every other exchange is left without a listing (fail-open).
    with_listing_store_mut(|store| {
        *store = Default::default();
        store.accept(
            gated.name(),
            ListedPairs {
                bases: BTreeSet::from(["BTC".to_string()]),
                total_markets: 300,
            },
            now_secs,
        );
    });

    // ICP is absent from the gated exchange's listing, so it is dropped; the
    // listing-less exchanges fail open and are kept.
    let icp = super::exchanges_listing_base_against_usdt(&exchanges, "ICP", now_secs);
    assert!(!icp.iter().any(|e| e.name() == gated.name()));
    assert_eq!(icp.len(), exchanges.len() - 1);

    // BTC is listed on the gated exchange, so the full set is queried.
    let btc = super::exchanges_listing_base_against_usdt(&exchanges, "BTC", now_secs);
    assert_eq!(btc.len(), exchanges.len());
}
