use async_trait::async_trait;
use candid::{encode_one, Principal};
use ic_xrc_types::{Asset, AssetClass, GetExchangeRateRequest, GetExchangeRateResult};
use std::cell::Cell;
use xrc::XRC_REQUEST_CYCLES_COST;

use crate::{
    state::{with_config, with_entries},
    types::{CallError, Entry, EntryResult},
    Environment,
};

const ONE_MINUTE_SECONDS: u64 = 60;
const NANOS_PER_SEC: u64 = 1_000_000_000;

thread_local! {
    static NEXT_CALL_AT_TIMESTAMP: Cell<u64> = Cell::new(0);
    static IS_CALLING_XRC: Cell<bool>  = Cell::new(false);
}

fn is_calling_xrc() -> bool {
    IS_CALLING_XRC.with(|c| c.get())
}

fn set_is_calling_xrc(is_calling: bool) {
    IS_CALLING_XRC.with(|c| c.set(is_calling));
}

fn next_call_at_timestamp() -> u64 {
    NEXT_CALL_AT_TIMESTAMP.with(|c| c.get())
}

fn set_next_call_at_timestamp(timestamp: u64) {
    NEXT_CALL_AT_TIMESTAMP.with(|c| c.set(timestamp))
}

#[async_trait]
trait Xrc {
    async fn get_exchange_rate(
        &self,
        request: GetExchangeRateRequest,
    ) -> Result<GetExchangeRateResult, CallError>;
}

struct XrcImpl {
    canister_id: Principal,
}

impl XrcImpl {
    fn new() -> Self {
        Self {
            canister_id: with_config(|config| config.xrc_canister_id),
        }
    }
}

#[async_trait]
impl Xrc for XrcImpl {
    async fn get_exchange_rate(
        &self,
        request: GetExchangeRateRequest,
    ) -> Result<GetExchangeRateResult, CallError> {
        ic_cdk::api::call::call_with_payment::<_, (GetExchangeRateResult,)>(
            self.canister_id,
            "get_exchange_rate",
            (request.clone(),),
            XRC_REQUEST_CYCLES_COST,
        )
        .await
        .map(|result| result.0)
        .map_err(|(rejection_code, err)| CallError {
            rejection_code,
            err,
        })
    }
}

pub(crate) fn beat(env: &impl Environment) {
    let now_secs = ((env.time() / NANOS_PER_SEC) / 60) * 60;
    let xrc_impl = XrcImpl::new();
    ic_cdk::spawn(call_xrc(xrc_impl, now_secs))
}

/// The function makes all of the GetExchangeRateRequests for the following asset pairs:
/// * ICP/CXDR
/// * BTC/BTT
/// * ETH/EUR
/// * SHIB/BTC
fn make_get_exchange_rate_requests(timestamp: u64) -> Vec<GetExchangeRateRequest> {
    vec![
        make_get_exchange_rate_request(
            Asset {
                symbol: "ICP".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            Asset {
                symbol: "CXDR".to_string(),
                class: AssetClass::FiatCurrency,
            },
            timestamp,
        ),
        make_get_exchange_rate_request(
            Asset {
                symbol: "BTC".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            Asset {
                symbol: "BTT".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            timestamp,
        ),
        make_get_exchange_rate_request(
            Asset {
                symbol: "ETH".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            Asset {
                symbol: "EUR".to_string(),
                class: AssetClass::FiatCurrency,
            },
            timestamp,
        ),
        make_get_exchange_rate_request(
            Asset {
                symbol: "SHIB".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            Asset {
                symbol: "BTC".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            timestamp,
        ),
    ]
}

fn make_get_exchange_rate_request(
    base_asset: Asset,
    quote_asset: Asset,
    timestamp: u64,
) -> GetExchangeRateRequest {
    GetExchangeRateRequest {
        base_asset,
        quote_asset,
        timestamp: Some(timestamp),
    }
}

async fn call_xrc(xrc_impl: impl Xrc, now_secs: u64) {
    if is_calling_xrc() {
        return;
    }

    if now_secs < next_call_at_timestamp() {
        return;
    }

    set_is_calling_xrc(true);

    // Request the rate from one minute ago * the current sample schedule value (this is done to ensure we do actually receive some rates).
    let one_minute_ago_secs = now_secs.saturating_sub(ONE_MINUTE_SECONDS);
    let requests = make_get_exchange_rate_requests(one_minute_ago_secs);
    for request in requests {
        let call_result = xrc_impl.get_exchange_rate(request.clone()).await;
        let result = match call_result {
            Ok(get_exchange_result) => match get_exchange_result {
                Ok(rate) => EntryResult::Rate(rate),
                Err(err) => EntryResult::RateError(err),
            },
            Err(err) => EntryResult::CallError(err),
        };

        let entry = Entry { request, result };
        let bytes = match encode_one(entry) {
            Ok(bytes) => bytes,
            Err(_) => {
                ic_cdk::println!("Failed to encode Entry");
                return;
            }
        };

        with_entries(|entries| {
            if let Err(err) = entries.append(&bytes) {
                ic_cdk::println!("No more space to append results: {:?}", err);
            }
        });
    }

    set_is_calling_xrc(false);

    set_next_call_at_timestamp(now_secs.saturating_add(5 * ONE_MINUTE_SECONDS));
}

#[cfg(test)]
mod test {

    use std::sync::{Arc, RwLock};

    use candid::Nat;
    use futures::FutureExt;
    use ic_cdk::api::call::RejectionCode;
    use ic_xrc_types::{ExchangeRate, ExchangeRateError, ExchangeRateMetadata};

    use crate::{api, environment::test::TestEnvironment, types::GetEntriesRequest};

    use super::*;

    /// Used to simulate calls to the exchange rate canister.
    #[derive(Default)]
    struct TestXrcImpl {
        responses: Vec<Result<GetExchangeRateResult, CallError>>,
        calls: RwLock<Vec<GetExchangeRateRequest>>,
    }

    impl TestXrcImpl {
        fn builder() -> TestXrcImplBuilder {
            TestXrcImplBuilder::new()
        }
    }

    struct TestXrcImplBuilder {
        r#impl: TestXrcImpl,
    }

    impl TestXrcImplBuilder {
        fn new() -> Self {
            Self {
                r#impl: TestXrcImpl::default(),
            }
        }

        /// Sets the responses for when [CallExchanges::get_cryptocurrency_usdt_rate] is called.
        fn with_responses(
            mut self,
            responses: Vec<Result<GetExchangeRateResult, CallError>>,
        ) -> Self {
            self.r#impl.responses = responses;
            self
        }

        /// Returns the built implmentation.
        fn build(self) -> TestXrcImpl {
            self.r#impl
        }
    }

    #[async_trait]
    impl Xrc for Arc<TestXrcImpl> {
        async fn get_exchange_rate(
            &self,
            request: GetExchangeRateRequest,
        ) -> Result<GetExchangeRateResult, CallError> {
            self.calls.write().unwrap().push(request);
            let length = self.calls.read().unwrap().len();
            self.responses
                .get(length - 1)
                .cloned()
                .expect("Missing a response for a call")
        }
    }

    #[test]
    fn call_xrc_can_retrieve_a_rate() {
        let env = TestEnvironment::builder().build();
        let request = make_get_exchange_rate_request(
            Asset {
                symbol: "ICP".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            Asset {
                symbol: "CXDR".to_string(),
                class: AssetClass::FiatCurrency,
            },
            0,
        );
        let timestamp_secs = 1;
        let rate = ExchangeRate {
            base_asset: Asset {
                symbol: "ICP".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            quote_asset: Asset {
                symbol: "CXDR".to_string(),
                class: AssetClass::FiatCurrency,
            },
            timestamp: timestamp_secs,
            rate: 1_000_000_000,
            metadata: ExchangeRateMetadata {
                decimals: 9,
                base_asset_num_queried_sources: 6,
                base_asset_num_received_rates: 6,
                quote_asset_num_queried_sources: 6,
                quote_asset_num_received_rates: 6,
                standard_deviation: 1,
                forex_timestamp: Some(timestamp_secs),
            },
        };
        let xrc = Arc::new(
            TestXrcImpl::builder()
                .with_responses(vec![
                    Ok(Ok(rate.clone())),
                    Ok(Ok(rate.clone())),
                    Ok(Ok(rate.clone())),
                    Ok(Ok(rate.clone())),
                ])
                .build(),
        );

        call_xrc(xrc.clone(), timestamp_secs)
            .now_or_never()
            .expect("future failed");

        let get_entries_response = api::get_entries(
            &env,
            GetEntriesRequest {
                offset: Nat::from(0),
                limit: Some(Nat::from(4)),
            },
        );

        // Check that `xrc` was called
        xrc.calls
            .read()
            .map(|calls| {
                let call = calls.get(0).expect("there should be 1 call");
                assert_eq!(call.base_asset, request.base_asset);
                assert_eq!(call.quote_asset, request.quote_asset);
                assert_eq!(call.timestamp, request.timestamp);
            })
            .expect("failed to read calls");

        // Check the total
        assert_eq!(get_entries_response.total, 4);

        // Check the request
        assert_eq!(
            get_entries_response.entries[0].request.base_asset,
            request.base_asset
        );
        assert_eq!(
            get_entries_response.entries[0].request.quote_asset,
            request.quote_asset
        );
        assert_eq!(
            get_entries_response.entries[0].request.timestamp,
            request.timestamp
        );

        // Check the result
        match &get_entries_response.entries[0].result {
            EntryResult::Rate(found_rate) => {
                assert_eq!(found_rate, &rate);
            }
            _ => panic!("Expected a rate to be found"),
        };
    }

    #[test]
    fn call_xrc_can_retrieve_a_rate_error() {
        let env = TestEnvironment::builder().build();
        let request = make_get_exchange_rate_request(
            Asset {
                symbol: "ICP".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            Asset {
                symbol: "CXDR".to_string(),
                class: AssetClass::FiatCurrency,
            },
            0,
        );
        let timestamp_secs = 1;
        let xrc = Arc::new(
            TestXrcImpl::builder()
                .with_responses(vec![
                    Ok(Err(ExchangeRateError::NotEnoughCycles)),
                    Ok(Err(ExchangeRateError::NotEnoughCycles)),
                    Ok(Err(ExchangeRateError::NotEnoughCycles)),
                    Ok(Err(ExchangeRateError::NotEnoughCycles)),
                ])
                .build(),
        );

        call_xrc(xrc.clone(), timestamp_secs)
            .now_or_never()
            .expect("future failed");

        let get_entries_response = api::get_entries(
            &env,
            GetEntriesRequest {
                offset: Nat::from(0),
                limit: Some(Nat::from(4)),
            },
        );

        // Check that `xrc` was called
        xrc.calls
            .read()
            .map(|calls| {
                let call = calls.get(0).expect("there should be 1 call");
                assert_eq!(call.base_asset, request.base_asset);
                assert_eq!(call.quote_asset, request.quote_asset);
                assert_eq!(call.timestamp, request.timestamp);
            })
            .expect("failed to read calls");

        // Check the total
        assert_eq!(get_entries_response.total, 4);

        // Check the request
        assert_eq!(
            get_entries_response.entries[0].request.base_asset,
            request.base_asset
        );
        assert_eq!(
            get_entries_response.entries[0].request.quote_asset,
            request.quote_asset
        );
        assert_eq!(
            get_entries_response.entries[0].request.timestamp,
            request.timestamp
        );

        // Check the result
        assert!(matches!(
            get_entries_response.entries[0].result,
            EntryResult::RateError(ExchangeRateError::NotEnoughCycles)
        ));
    }

    #[test]
    fn call_xrc_can_receive_a_call_error() {
        let err = "Failed to call canister".to_string();
        let env = TestEnvironment::builder().build();
        let request = make_get_exchange_rate_request(
            Asset {
                symbol: "ICP".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            Asset {
                symbol: "CXDR".to_string(),
                class: AssetClass::FiatCurrency,
            },
            0,
        );
        let timestamp_secs = 1;
        let xrc = Arc::new(
            TestXrcImpl::builder()
                .with_responses(vec![
                    Err(CallError {
                        rejection_code: RejectionCode::CanisterError,
                        err: err.clone(),
                    }),
                    Err(CallError {
                        rejection_code: RejectionCode::CanisterError,
                        err: err.clone(),
                    }),
                    Err(CallError {
                        rejection_code: RejectionCode::CanisterError,
                        err: err.clone(),
                    }),
                    Err(CallError {
                        rejection_code: RejectionCode::CanisterError,
                        err: err.clone(),
                    }),
                ])
                .build(),
        );

        call_xrc(xrc.clone(), timestamp_secs)
            .now_or_never()
            .expect("future failed");

        let get_entries_response = api::get_entries(
            &env,
            GetEntriesRequest {
                offset: Nat::from(0),
                limit: Some(Nat::from(4)),
            },
        );

        // Check that `xrc` was called
        xrc.calls
            .read()
            .map(|calls| {
                let call = calls.get(0).expect("there should be 1 call");
                assert_eq!(call.base_asset, request.base_asset);
                assert_eq!(call.quote_asset, request.quote_asset);
                assert_eq!(call.timestamp, request.timestamp);
            })
            .expect("failed to read calls");

        // Check the total
        assert_eq!(get_entries_response.total, 4);

        // Check the request
        assert_eq!(
            get_entries_response.entries[0].request.base_asset,
            request.base_asset
        );
        assert_eq!(
            get_entries_response.entries[0].request.quote_asset,
            request.quote_asset
        );
        assert_eq!(
            get_entries_response.entries[0].request.timestamp,
            request.timestamp
        );

        // Check the result
        assert!(matches!(
            &get_entries_response.entries[0].result,
            EntryResult::CallError(call_error) if call_error.rejection_code == RejectionCode::CanisterError && call_error.err == err
        ));
    }
}
