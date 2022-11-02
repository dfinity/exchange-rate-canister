use std::{collections::HashMap, sync::RwLock};

use async_trait::async_trait;
use candid::Principal;
use futures::FutureExt;
use maplit::hashmap;

use crate::{
    candid::{Asset, AssetClass, ExchangeRateError, GetExchangeRateRequest},
    environment::test::TestEnvironment,
    CallExchangeError, QueriedExchangeRate, CYCLES_MINTING_CANISTER_ID, EXCHANGES,
    XRC_REQUEST_CYCLES_COST,
};

use super::{get_exchange_rate_internal, CallExchanges};

#[derive(Default)]
struct TestCallExchangesImpl {
    get_cryptocurrency_usdt_rate_responses:
        HashMap<String, Result<QueriedExchangeRate, CallExchangeError>>,
    _get_stablecoin_rates_responses: HashMap<Asset, Result<QueriedExchangeRate, CallExchangeError>>,
    calls: RwLock<Vec<(Asset, u64)>>,
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

    fn with_get_cryptocurrency_usdt_rate_responses(
        mut self,
        responses: HashMap<String, Result<QueriedExchangeRate, CallExchangeError>>,
    ) -> Self {
        self.r#impl.get_cryptocurrency_usdt_rate_responses = responses;
        self
    }

    #[allow(dead_code)]
    fn with_get_stablecoin_rates_responses(
        mut self,
        responses: HashMap<Asset, Result<QueriedExchangeRate, CallExchangeError>>,
    ) -> Self {
        self.r#impl._get_stablecoin_rates_responses = responses;
        self
    }

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
        self.calls.write().unwrap().push((asset.clone(), timestamp));
        self.get_cryptocurrency_usdt_rate_responses
            .get(&asset.symbol)
            .cloned()
            .unwrap_or(Err(CallExchangeError::NoRatesFound))
    }

    async fn get_stablecoin_rates(
        &self,
        _: &[&str],
        _: u64,
    ) -> Vec<Result<QueriedExchangeRate, CallExchangeError>> {
        todo!()
    }
}

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
        &[1, 2, 3],
        EXCHANGES.len(),
        3,
    )
}

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
        &[1, 2, 3],
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
    let caller = Principal::anonymous();
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

    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, caller, request)
        .now_or_never()
        .expect("future should complete");
    assert!(matches!(result, Err(ExchangeRateError::NotEnoughCycles)));
}

/// This function tests that [get_exchange_rate] will return an [ExchangeRateError::FailedToAcceptCycles]
/// when the canister fails to accept the cycles sent by the caller.
#[test]
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
    let caller = Principal::anonymous();
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

    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, caller, request)
        .now_or_never()
        .expect("future should complete");

    assert!(matches!(
        result,
        Err(ExchangeRateError::FailedToAcceptCycles)
    ));
}

/// This function tests [get_exchange_rate] does not charge the cycles minting canister for usage.
#[test]
fn get_exchange_rate_will_not_charge_cycles_if_caller_is_cmc() {
    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(hashmap! {
            "BTC".to_string() => Ok(btc_queried_exchange_rate_mock()),
            "ICP".to_string() => Ok(icp_queried_exchange_rate_mock())
        })
        .build();
    let env = TestEnvironment::builder().with_cycles_available(0).build();
    let caller = CYCLES_MINTING_CANISTER_ID;
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

    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, caller, request)
        .now_or_never()
        .expect("future should complete");
    assert!(matches!(result, Ok(_)));
    assert_eq!(call_exchanges_impl.calls.read().unwrap().len(), 2);
}
