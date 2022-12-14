use std::{collections::HashMap, sync::RwLock};

use async_trait::async_trait;
use futures::FutureExt;
use maplit::hashmap;

use crate::{
    candid::{Asset, AssetClass, ExchangeRateError, GetExchangeRateRequest},
    environment::test::TestEnvironment,
    with_cache_mut, CallExchangeError, QueriedExchangeRate, CACHE_RETENTION_PERIOD_SEC,
    CYCLES_MINTING_CANISTER_ID, EXCHANGES, XRC_BASE_CYCLES_COST,
    XRC_OUTBOUND_HTTP_CALL_CYCLES_COST, XRC_REQUEST_CYCLES_COST,
};

use super::{get_exchange_rate_internal, CallExchanges};

/// Used to simulate HTTP outcalls from the canister for testing purposes.
#[derive(Default)]
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

/// A simple mock BTC/USDT [QueriedExchangeRate].
fn btc_queried_exchange_rate_mock() -> QueriedExchangeRate {
    QueriedExchangeRate::new(
        Asset {
            symbol: "BTC".to_string(),
            class: AssetClass::Cryptocurrency,
        },
        Asset {
            symbol: "USDT".to_string(),
            class: AssetClass::Cryptocurrency,
        },
        0,
        &[101, 102, 103],
        EXCHANGES.len(),
        3,
    )
}

/// A simple mock ICP/USDT [QueriedExchangeRate].
fn icp_queried_exchange_rate_mock() -> QueriedExchangeRate {
    QueriedExchangeRate::new(
        Asset {
            symbol: "ICP".to_string(),
            class: AssetClass::Cryptocurrency,
        },
        Asset {
            symbol: "USDT".to_string(),
            class: AssetClass::Cryptocurrency,
        },
        0,
        &[101, 102, 103],
        EXCHANGES.len(),
        3,
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
    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(hashmap! {
            "BTC".to_string() => Ok(btc_queried_exchange_rate_mock()),
            "ICP".to_string() => Ok(icp_queried_exchange_rate_mock())
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

/// This function tests that [get_exchange_rate] charges the full cycles fee for usage when the cache does not
/// contain the necessary entries.
#[test]
fn get_exchange_rate_will_charge_cycles() {
    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(hashmap! {
            "BTC".to_string() => Ok(btc_queried_exchange_rate_mock()),
            "ICP".to_string() => Ok(icp_queried_exchange_rate_mock())
        })
        .build();
    let env = TestEnvironment::builder()
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(5_000_000_000)
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

/// This function tests that [get_exchange_rate] charges the base cycles cost for usage.
#[test]
fn get_exchange_rate_will_charge_the_base_cost_worth_of_cycles() {
    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(hashmap! {
            "BTC".to_string() => Ok(btc_queried_exchange_rate_mock()),
            "ICP".to_string() => Ok(icp_queried_exchange_rate_mock())
        })
        .build();
    let env = TestEnvironment::builder()
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_BASE_CYCLES_COST)
        .build();
    with_cache_mut(|cache| {
        cache.insert(
            btc_queried_exchange_rate_mock(),
            0,
            CACHE_RETENTION_PERIOD_SEC,
        );
        cache.insert(
            icp_queried_exchange_rate_mock(),
            0,
            CACHE_RETENTION_PERIOD_SEC,
        );
    });

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
        0
    );
}

/// This function tests that [get_exchange_rate] charges the base cycles cost plus the cost of a single exchange rate lookup when there
/// is only one entry found in the cache.
#[test]
fn get_exchange_rate_will_charge_the_base_cost_plus_outbound_cycles_worth_of_cycles_when_cache_contains_one_entry(
) {
    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(hashmap! {
            "BTC".to_string() => Ok(btc_queried_exchange_rate_mock()),
            "ICP".to_string() => Ok(icp_queried_exchange_rate_mock())
        })
        .build();
    let env = TestEnvironment::builder()
        .with_cycles_available(XRC_REQUEST_CYCLES_COST)
        .with_accepted_cycles(XRC_BASE_CYCLES_COST + XRC_OUTBOUND_HTTP_CALL_CYCLES_COST)
        .build();
    with_cache_mut(|cache| {
        cache.insert(
            btc_queried_exchange_rate_mock(),
            0,
            CACHE_RETENTION_PERIOD_SEC,
        );
    });

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
        1
    );
}
