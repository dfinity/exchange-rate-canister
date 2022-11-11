use std::{collections::HashMap, sync::RwLock};

use async_trait::async_trait;
use futures::FutureExt;
use maplit::hashmap;

use crate::{
    candid::{Asset, AssetClass, ExchangeRateError, GetExchangeRateRequest},
    environment::test::TestEnvironment,
    with_cache, with_cache_mut, CallExchangeError, QueriedExchangeRate, CACHE_RETENTION_PERIOD_SEC,
    CYCLES_MINTING_CANISTER_ID, EXCHANGES, XRC_REQUEST_CYCLES_COST,
};

use super::{get_exchange_rate_internal, CallExchanges};

/// Used to simulate HTTP outcalls from the canister for testing purposes.
#[derive(Default, Debug)]
struct TestCallExchangesImpl {
    /// Contains the responses when [CallExchanges::get_cryptocurrency_usdt_rate] is called.
    get_cryptocurrency_usdt_rate_responses:
        HashMap<String, Result<QueriedExchangeRate, CallExchangeError>>,
    /// The received [CallExchanges::get_cryptocurrency_usdt_rate] calls from the test.
    get_cryptocurrency_usdt_rate_calls: RwLock<Vec<(Asset, u64)>>,
    /// Contains the responses when [CallExchanges::get_stablecoin_rates] is called.
    _get_stablecoin_rates_responses:
        HashMap<Vec<String>, Vec<Result<QueriedExchangeRate, CallExchangeError>>>,
    /// The received [CallExchanges::get_cryptocurrency_usdt_rate] calls from the test.
    _get_stablecoin_rates_calls: RwLock<Vec<(Vec<String>, u64)>>,
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
        responses: HashMap<String, Result<QueriedExchangeRate, CallExchangeError>>,
    ) -> Self {
        self.r#impl.get_cryptocurrency_usdt_rate_responses = responses;
        self
    }

    /// Sets the responses for when [CallExchanges::get_stablecoin_rates] is called.
    #[allow(dead_code)]
    fn with_get_stablecoin_rates_responses(
        mut self,
        responses: HashMap<Vec<String>, Vec<Result<QueriedExchangeRate, CallExchangeError>>>,
    ) -> Self {
        self.r#impl._get_stablecoin_rates_responses = responses;
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
        asset: &Asset,
        timestamp: u64,
    ) -> Result<QueriedExchangeRate, CallExchangeError> {
        self.get_cryptocurrency_usdt_rate_calls
            .write()
            .unwrap()
            .push((asset.clone(), timestamp));
        self.get_cryptocurrency_usdt_rate_responses
            .get(&asset.symbol)
            .cloned()
            .unwrap_or(Err(CallExchangeError::NoRatesFound))
    }

    async fn get_stablecoin_rates(
        &self,
        assets: &[&str],
        timestamp: u64,
    ) -> Vec<Result<QueriedExchangeRate, CallExchangeError>> {
        let assets_vec = assets.iter().map(|a| a.to_string()).collect::<Vec<_>>();
        self._get_stablecoin_rates_calls
            .write()
            .unwrap()
            .push((assets_vec.clone(), timestamp));
        self._get_stablecoin_rates_responses
            .get(&assets_vec)
            .cloned()
            .unwrap_or_default()
    }
}

/// A simple method to make quick mock cryptocurrency exchange rates.
fn mock_cryptocurrency_exchange_rate(
    symbol: &str,
    rates: &[u64],
    timestamp: u64,
) -> QueriedExchangeRate {
    QueriedExchangeRate::new(
        Asset {
            symbol: symbol.to_string(),
            class: AssetClass::Cryptocurrency,
        },
        Asset {
            symbol: "USDT".to_string(),
            class: AssetClass::Cryptocurrency,
        },
        timestamp,
        rates,
        EXCHANGES.len(),
        rates.len(),
    )
}

/// This function tests that [get_exchange_rate] will return an [ExchangeRateError::NotEnoughCycles]
/// when not enough cycles are sent by the caller.
#[test]
fn get_exchange_rate_fails_when_not_enough_cycles() {
    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(hashmap! {
            "BTC".to_string() => Ok(QueriedExchangeRate::default()),
            "ICP".to_string() => Ok(QueriedExchangeRate::default())
        })
        .build();
    let env = TestEnvironment::builder().with_cycles_available(0).build();
    let request = GetExchangeRateRequest {
        base_asset: Asset {
            symbol: "BTC".to_string(),
            class: AssetClass::Cryptocurrency,
        },
        quote_asset: Asset {
            symbol: "ICP".to_string(),
            class: AssetClass::Cryptocurrency,
        },
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
        .with_get_cryptocurrency_usdt_rate_responses(hashmap! {
            "BTC".to_string() => Ok(QueriedExchangeRate::default()),
            "ICP".to_string() => Ok(QueriedExchangeRate::default())
        })
        .build();
    let env = TestEnvironment::builder()
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(0)
        .build();
    let request = GetExchangeRateRequest {
        base_asset: Asset {
            symbol: "EUR".to_string(),
            class: AssetClass::FiatCurrency,
        },
        quote_asset: Asset {
            symbol: "USD".to_string(),
            class: AssetClass::FiatCurrency,
        },
        timestamp: None,
    };

    get_exchange_rate_internal(&env, &call_exchanges_impl, &request).now_or_never();
}

/// This function tests that [get_exchange_rate] does not charge the cycles minting canister for usage.
#[test]
fn get_exchange_rate_will_not_charge_cycles_if_caller_is_cmc() {
    let timestamp = 0;
    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(hashmap! {
            "BTC".to_string() => Ok(mock_cryptocurrency_exchange_rate("BTC", &[1, 2, 3], timestamp)),
            "ICP".to_string() => Ok(mock_cryptocurrency_exchange_rate("ICP", &[1, 2, 3], timestamp))
        })
        .build();
    let env = TestEnvironment::builder()
        .with_cycles_available(0)
        .with_caller(CYCLES_MINTING_CANISTER_ID)
        .build();
    let request = GetExchangeRateRequest {
        base_asset: Asset {
            symbol: "BTC".to_string(),
            class: AssetClass::Cryptocurrency,
        },
        quote_asset: Asset {
            symbol: "ICP".to_string(),
            class: AssetClass::Cryptocurrency,
        },
        timestamp: Some(0),
    };

    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");
    assert!(matches!(result, Ok(_)));
    assert_eq!(
        call_exchanges_impl
            .get_cryptocurrency_usdt_rate_calls
            .read()
            .unwrap()
            .len(),
        2
    );
}

/// This function tests [get_exchange_rate] does charge the cycles minting canister for usage.
#[test]
fn get_exchange_rate_will_charge_cycles() {
    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(hashmap! {
            "BTC".to_string() => Ok(mock_cryptocurrency_exchange_rate("BTC", &[1, 2, 3], 0)),
            "ICP".to_string() => Ok(mock_cryptocurrency_exchange_rate("ICP", &[1, 2, 3], 0))
        })
        .build();
    let env = TestEnvironment::builder()
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_REQUEST_CYCLES_COST)
        .build();
    let request = GetExchangeRateRequest {
        base_asset: Asset {
            symbol: "BTC".to_string(),
            class: AssetClass::Cryptocurrency,
        },
        quote_asset: Asset {
            symbol: "ICP".to_string(),
            class: AssetClass::Cryptocurrency,
        },
        timestamp: None,
    };

    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete");
    assert!(matches!(result, Ok(_)));
    assert_eq!(
        call_exchanges_impl
            .get_cryptocurrency_usdt_rate_calls
            .read()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn handle_cryptocurrency_pair_can_successfully_return_a_rate_by_calling_exchanges() {
    let expected_normalized_timestamp = 1668027960;
    let timestamp = 1668027974;
    let btc_mock_rate =
        mock_cryptocurrency_exchange_rate("BTC", &[200, 210, 205], expected_normalized_timestamp);
    let icp_mock_rate =
        mock_cryptocurrency_exchange_rate("ICP", &[100, 105, 110], expected_normalized_timestamp);
    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(hashmap! {
            "BTC".to_string() => Ok(btc_mock_rate.clone()),
            "ICP".to_string() => Ok(icp_mock_rate.clone())
        })
        .build();
    let env = TestEnvironment::builder()
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_REQUEST_CYCLES_COST)
        .with_time_secs(expected_normalized_timestamp)
        .build();
    let request = GetExchangeRateRequest {
        base_asset: Asset {
            symbol: "BTC".to_string(),
            class: AssetClass::Cryptocurrency,
        },
        quote_asset: Asset {
            symbol: "ICP".to_string(),
            class: AssetClass::Cryptocurrency,
        },
        timestamp: Some(timestamp),
    };

    assert_eq!(with_cache(|cache| cache.size()), 0, "cache should be empty");

    let rate = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete")
        .expect("should be able to successfully retrieve a rate");

    let calls = call_exchanges_impl
        .get_cryptocurrency_usdt_rate_calls
        .read()
        .expect("should be able to read calls");

    let first_call = calls.get(0).expect("should contain at least 1 call");
    assert_eq!(first_call.0.symbol, "BTC");
    assert_eq!(first_call.0.class, AssetClass::Cryptocurrency);
    assert_eq!(first_call.1, expected_normalized_timestamp);

    let second_call = calls.get(1).expect("should contain at least 2 calls");
    assert_eq!(second_call.0.symbol, "ICP");
    assert_eq!(second_call.0.class, AssetClass::Cryptocurrency);
    assert_eq!(second_call.1, expected_normalized_timestamp);

    assert_eq!(rate.timestamp, expected_normalized_timestamp);
    assert_eq!(
        rate.metadata.base_asset_num_queried_sources,
        EXCHANGES.len()
    );
    assert_eq!(rate.metadata.base_asset_num_received_rates, 3);
    assert_eq!(
        rate.metadata.quote_asset_num_queried_sources,
        EXCHANGES.len()
    );
    assert_eq!(rate.metadata.quote_asset_num_received_rates, 3);
    assert_eq!(rate.metadata.standard_deviation_permyriad, 855);
    assert_eq!(rate.rate_permyriad, 19523);

    with_cache_mut(|cache| {
        let rate = cache
            .get("ICP", expected_normalized_timestamp, timestamp)
            .expect("ICP rate should be in the cache");
        assert_eq!(rate, icp_mock_rate);

        let rate = cache
            .get("BTC", expected_normalized_timestamp, timestamp)
            .expect("BTC rate should be in the cache");
        assert_eq!(rate, btc_mock_rate);
    });
}

#[test]
fn handle_cryptocurrency_pair_can_successfully_return_a_rate_by_using_the_cache() {
    let expected_normalized_timestamp = 1668027960;
    let timestamp = 1668027974;
    let btc_mock_rate =
        mock_cryptocurrency_exchange_rate("BTC", &[200, 210, 205], expected_normalized_timestamp);
    let icp_mock_rate =
        mock_cryptocurrency_exchange_rate("ICP", &[100, 105, 110], expected_normalized_timestamp);
    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(hashmap! {
            "BTC".to_string() => Ok(btc_mock_rate.clone()),
            "ICP".to_string() => Ok(icp_mock_rate.clone())
        })
        .build();
    let env = TestEnvironment::builder()
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_REQUEST_CYCLES_COST)
        .with_time_secs(expected_normalized_timestamp)
        .build();
    let request = GetExchangeRateRequest {
        base_asset: Asset {
            symbol: "BTC".to_string(),
            class: AssetClass::Cryptocurrency,
        },
        quote_asset: Asset {
            symbol: "ICP".to_string(),
            class: AssetClass::Cryptocurrency,
        },
        timestamp: Some(timestamp),
    };

    with_cache_mut(|cache| {
        cache.insert(
            btc_mock_rate.clone(),
            expected_normalized_timestamp,
            CACHE_RETENTION_PERIOD_SEC,
        );
        cache.insert(
            icp_mock_rate.clone(),
            expected_normalized_timestamp,
            CACHE_RETENTION_PERIOD_SEC,
        );
    });

    let rate = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
        .now_or_never()
        .expect("future should complete")
        .expect("should be able to successfully retrieve a rate");

    let calls = call_exchanges_impl
        .get_cryptocurrency_usdt_rate_calls
        .read()
        .expect("should be able to read calls");
    assert_eq!(calls.len(), 0);

    assert_eq!(rate.timestamp, expected_normalized_timestamp);
    assert_eq!(
        rate.metadata.base_asset_num_queried_sources,
        EXCHANGES.len()
    );
    assert_eq!(rate.metadata.base_asset_num_received_rates, 3);
    assert_eq!(
        rate.metadata.quote_asset_num_queried_sources,
        EXCHANGES.len()
    );
    assert_eq!(rate.metadata.quote_asset_num_received_rates, 3);
    assert_eq!(rate.metadata.standard_deviation_permyriad, 855);
    assert_eq!(rate.rate_permyriad, 19523);

    with_cache_mut(|cache| {
        let rate = cache
            .get("ICP", expected_normalized_timestamp, timestamp)
            .expect("ICP rate should be in the cache");
        assert_eq!(rate, icp_mock_rate);

        let rate = cache
            .get("BTC", expected_normalized_timestamp, timestamp)
            .expect("BTC rate should be in the cache");
        assert_eq!(rate, btc_mock_rate);
    });
}
