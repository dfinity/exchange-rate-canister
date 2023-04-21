use std::{collections::HashMap, sync::RwLock};

use async_trait::async_trait;
use futures::FutureExt;
use ic_xrc_types::{Asset, AssetClass, ExchangeRateError, GetExchangeRateRequest};
use maplit::hashmap;

use crate::{
    environment::test::TestEnvironment,
    inflight::test::set_inflight_tracking,
    rate_limiting::test::{set_request_counter, REQUEST_COUNTER_TRIGGER_RATE_LIMIT},
    usdt_asset, with_cache_mut, with_forex_rate_store_mut, CallExchangeError, QueriedExchangeRate,
    DAI, EXCHANGES, PRIVILEGED_CANISTER_IDS, RATE_UNIT, USDC, XRC_BASE_CYCLES_COST,
    XRC_IMMEDIATE_REFUND_CYCLES, XRC_MINIMUM_FEE_COST, XRC_OUTBOUND_HTTP_CALL_CYCLES_COST,
    XRC_REQUEST_CYCLES_COST,
};

use super::{get_exchange_rate_internal, CallExchanges};
use crate::api::usd_asset;

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
fn btc_asset() -> Asset {
    Asset {
        symbol: "BTC".to_string(),
        class: AssetClass::Cryptocurrency,
    }
}

/// Used to simulate HTTP outcalls from the canister for testing purposes.
#[derive(Default)]
struct TestCallExchangesImpl {
    /// Contains the responses when [CallExchanges::get_cryptocurrency_usdt_rate] is called.
    get_cryptocurrency_usdt_rate_responses:
        HashMap<String, Result<QueriedExchangeRate, CallExchangeError>>,
    /// The received [CallExchanges::get_cryptocurrency_usdt_rate] calls from the test.
    get_cryptocurrency_usdt_rate_calls: RwLock<Vec<(Asset, u64)>>,
    /// Contains the responses when [CallExchanges::get_stablecoin_rates] is called.
    get_stablecoin_rates_responses: HashMap<String, Result<QueriedExchangeRate, CallExchangeError>>,
    /// The received [CallExchanges::get_cryptocurrency_usdt_rate] calls from the test.
    get_stablecoin_rates_calls: RwLock<Vec<(Vec<String>, u64)>>,
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
    fn with_get_stablecoin_rates_responses(
        mut self,
        responses: HashMap<String, Result<QueriedExchangeRate, CallExchangeError>>,
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
        self.get_stablecoin_rates_calls
            .write()
            .unwrap()
            .push((assets_vec, timestamp));

        let mut results = vec![];
        for asset in assets {
            let entry = self
                .get_stablecoin_rates_responses
                .get(&asset.to_string())
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
        .with_get_cryptocurrency_usdt_rate_responses(hashmap! {
            "BTC".to_string() => Ok(btc_queried_exchange_rate_mock()),
            "ICP".to_string() => Ok(icp_queried_exchange_rate_mock())
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

/// This function tests that [get_exchange_rate] charges the rate limit fee for usage when there are too many HTTP outcalls.
#[test]
fn get_exchange_rate_will_charge_rate_limit_fee() {
    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(hashmap! {
            "BTC".to_string() => Ok(btc_queried_exchange_rate_mock()),
            "ICP".to_string() => Ok(icp_queried_exchange_rate_mock())
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
        .with_get_cryptocurrency_usdt_rate_responses(hashmap! {
            "ICP".to_string() => Ok(icp_queried_exchange_rate_mock())
        })
        .with_get_stablecoin_rates_responses(hashmap! {
            DAI.to_string() => Ok(stablecoin_mock(DAI, &[RATE_UNIT])),
            USDC.to_string() => Ok(stablecoin_mock(USDC, &[RATE_UNIT])),
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
        .with_get_cryptocurrency_usdt_rate_responses(hashmap! {
            "ICP".to_string() => Ok(icp_queried_exchange_rate_mock())
        })
        .with_get_stablecoin_rates_responses(hashmap! {
            DAI.to_string() => Ok(stablecoin_mock(DAI, &[RATE_UNIT])),
            USDC.to_string() => Ok(stablecoin_mock(USDC, &[RATE_UNIT])),
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

/// This function tests to ensure a rate is returned when asking for a
/// crypto/non-USD pair.
#[test]
fn get_exchange_rate_for_crypto_non_usd_pair() {
    with_forex_rate_store_mut(|store| {
        store.put(
            0,
            hashmap! {
                    "EUR".to_string() =>
                        QueriedExchangeRate::new(eur_asset(), usd_asset(), 0, &[800_000_000], 1, 1, Some(0))
            },
        );
    });

    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(hashmap! {
            "ICP".to_string() => Ok(icp_queried_exchange_rate_mock())
        })
        .with_get_stablecoin_rates_responses(hashmap! {
            DAI.to_string() => Ok(stablecoin_mock(DAI, &[RATE_UNIT])),
            USDC.to_string() => Ok(stablecoin_mock(USDC, &[RATE_UNIT])),
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
            hashmap! {
                    "EUR".to_string() =>
                        QueriedExchangeRate::new(eur_asset(), usd_asset(), 0, &[800_000_000], 1, 1, Some(0))
            },
        );
    });

    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(hashmap! {
            "ICP".to_string() => Ok(icp_queried_exchange_rate_mock())
        })
        .with_get_stablecoin_rates_responses(hashmap! {
            DAI.to_string() => Ok(stablecoin_mock(DAI, &[RATE_UNIT])),
            USDC.to_string() => Ok(stablecoin_mock(USDC, &[RATE_UNIT])),
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
            hashmap! {
                    "EUR".to_string() =>
                        QueriedExchangeRate::new(eur_asset(), usd_asset(), 0, &[800_000_000], 4, 4, Some(0))
            },
        );
    });

    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_stablecoin_rates_responses(hashmap! {
            DAI.to_string() => Ok(stablecoin_mock(DAI, &[RATE_UNIT])),
            USDC.to_string() => Ok(stablecoin_mock(USDC, &[RATE_UNIT])),
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
            hashmap! {
                    "EUR".to_string() =>
                        QueriedExchangeRate::new(eur_asset(), usd_asset(), 0, &[800_000_000], 1, 1, Some(0))
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
            hashmap! {
                    "EUR".to_string() =>
                        QueriedExchangeRate::new(eur_asset(), usd_asset(), 0, &[800_000_000], 1, 1, Some(0))
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
            hashmap! {
                    "EUR".to_string() =>
                        QueriedExchangeRate::new(eur_asset(), usd_asset(), 86_400, &[800_000_000], 4, 4, Some(0))
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
        .with_get_cryptocurrency_usdt_rate_responses(hashmap! {
            "BTC".to_string() => Ok(btc_queried_exchange_rate_mock()),
            "ICP".to_string() => Ok(icp_queried_exchange_rate_mock())
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
        .with_get_cryptocurrency_usdt_rate_responses(hashmap! {
            "BTC".to_string() => Ok(btc_queried_exchange_rate_mock()),
            "ICP".to_string() => Ok(icp_queried_exchange_rate_mock())
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
    assert!(matches!(result, Ok(_)));
}

/// This function tests that [get_exchange_rate] charges the maximum fee for usage when the request
/// contains ANY symbol-timestamp pairs that are not currently inflight.
#[test]
fn get_exchange_rate_will_retrieve_rates_if_inflight_tracking_contains_any_symbol_timestamp_pairs()
{
    set_inflight_tracking(vec!["AVAX".to_string(), "ICP".to_string()], 0);
    let call_exchanges_impl = TestCallExchangesImpl::builder()
        .with_get_cryptocurrency_usdt_rate_responses(hashmap! {
            "BTC".to_string() => Ok(btc_queried_exchange_rate_mock()),
            "ICP".to_string() => Ok(icp_queried_exchange_rate_mock())
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
        .with_get_cryptocurrency_usdt_rate_responses(hashmap! {
            "ICP".to_string() => Ok(icp_queried_exchange_rate_mock())
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
        .with_get_cryptocurrency_usdt_rate_responses(hashmap! {
            "ICP".to_string() => Ok(icp_queried_exchange_rate_mock())
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

mod privileged_callers_can_bypass_pending {

    use super::*;

    /// This function tests that [get_exchange_rate] allows privileged callers to bypass the pending check (crytpo pair).
    #[test]
    fn get_exchange_rate_will_allow_a_privileged_caller_to_bypass_pending_check_crypto_pair() {
        set_inflight_tracking(vec!["BTC".to_string(), "ICP".to_string()], 0);
        let call_exchanges_impl = TestCallExchangesImpl::builder()
            .with_get_cryptocurrency_usdt_rate_responses(hashmap! {
                "BTC".to_string() => Ok(btc_queried_exchange_rate_mock()),
                "ICP".to_string() => Ok(icp_queried_exchange_rate_mock())
            })
            .build();
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

        let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
            .now_or_never()
            .expect("future should complete");
        assert!(matches!(result, Ok(_)));
    }

    /// This function tests that [get_exchange_rate] allows privileged callers to bypass the pending check (crypto-fiat pair).
    #[test]
    fn get_exchange_rate_will_allow_a_privileged_caller_to_bypass_pending_check_crypto_fiat_pair() {
        set_inflight_tracking(vec!["BTC".to_string(), "ICP".to_string()], 0);
        let call_exchanges_impl = TestCallExchangesImpl::builder()
            .with_get_cryptocurrency_usdt_rate_responses(hashmap! {
                "ICP".to_string() => Ok(icp_queried_exchange_rate_mock())
            })
            .with_get_stablecoin_rates_responses(hashmap! {
                DAI.to_string() => Ok(stablecoin_mock(DAI, &[RATE_UNIT])),
                USDC.to_string() => Ok(stablecoin_mock(USDC, &[RATE_UNIT])),
            })
            .build();
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

        let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request)
            .now_or_never()
            .expect("future should complete");
        assert!(matches!(result, Ok(_)));
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
            .with_get_cryptocurrency_usdt_rate_responses(hashmap! {
                "BTC".to_string() => Ok(btc_queried_exchange_rate_mock()),
                "ICP".to_string() => Ok(icp_queried_exchange_rate_mock())
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
            .with_get_cryptocurrency_usdt_rate_responses(hashmap! {
                "BTC".to_string() => Ok(btc_queried_exchange_rate_mock()),
                "ICP".to_string() => Ok(icp_queried_exchange_rate_mock())
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
            cache.insert(&stablecoin_mock(DAI, &[RATE_UNIT]));
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
            cache.insert(&stablecoin_mock(DAI, &[RATE_UNIT]));
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
        .with_get_cryptocurrency_usdt_rate_responses(hashmap! {
            "ICP".to_string() => Ok(icp_queried_exchange_rate_mock())
        })
        .with_get_stablecoin_rates_responses(hashmap! {
            DAI.to_string() => Ok(stablecoin_mock(DAI, &[RATE_UNIT])),
            USDC.to_string() => Ok(stablecoin_mock(USDC, &[RATE_UNIT])),
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
        .with_get_cryptocurrency_usdt_rate_responses(hashmap! {
            "ICP".to_string() => Ok(icp_queried_exchange_rate_mock())
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
