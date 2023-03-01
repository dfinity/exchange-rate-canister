use std::time::Instant;

use xrc::{
    candid::{Asset, AssetClass, GetExchangeRateRequest, GetExchangeRateResult},
    EXCHANGES,
};

use crate::{
    container::{run_scenario, Container},
    tests::{build_response, get_sample_json_for_exchange},
};

/// This test is used to confirm that the exchange rate canister's cache is
/// able to contain requested rates to improve the time it takes to receive a
/// response from the `get_exchange_rate` endpoint.
#[ignore]
#[test]
fn can_successfully_cache_rates() {
    struct ScenarioResult {
        call_result_1: GetExchangeRateResult,
        time_passed_1_ms: u128,
        call_result_2: GetExchangeRateResult,
        time_passed_2_ms: u128,
    }

    let timestamp = 1614596340;
    let request = GetExchangeRateRequest {
        timestamp: Some(timestamp),
        quote_asset: Asset {
            symbol: "BTC".to_string(),
            class: AssetClass::Cryptocurrency,
        },
        base_asset: Asset {
            symbol: "ICP".to_string(),
            class: AssetClass::Cryptocurrency,
        },
    };

    let responses = EXCHANGES
        .iter()
        .flat_map(|exchange| {
            let json = get_sample_json_for_exchange(exchange);
            [
                build_response(exchange, &request.base_asset, timestamp, json.clone()),
                build_response(exchange, &request.quote_asset, timestamp, json),
            ]
        })
        .collect::<Vec<_>>();

    let container = Container::builder()
        .name("can_successfully_cache_rates")
        .exchange_responses(responses)
        .build();

    let request_1 = request.clone();
    let request_2 = request.clone();
    let scenario_result = run_scenario(container, |container: &Container| {
        let instant = Instant::now();
        let call_result_1 = container
            .call_canister::<_, GetExchangeRateResult>("get_exchange_rate", request_1)
            .expect("Failed to call canister for rates");
        let time_passed_1_ms = instant.elapsed().as_millis();

        let instant = Instant::now();
        let call_result_2 = container
            .call_canister::<_, GetExchangeRateResult>("get_exchange_rate", request_2)
            .expect("Failed to call canister for rates");
        let time_passed_2_ms = instant.elapsed().as_millis();
        Ok(ScenarioResult {
            call_result_1,
            time_passed_1_ms,
            call_result_2,
            time_passed_2_ms,
        })
    })
    .expect("Scenario failed");

    let exchange_rate = scenario_result
        .call_result_1
        .expect("Failed to retrieve an exchange rate from the canister.");
    assert_eq!(exchange_rate.base_asset, request.base_asset);
    assert_eq!(exchange_rate.quote_asset, request.quote_asset);
    assert_eq!(exchange_rate.timestamp, timestamp);
    assert_eq!(exchange_rate.metadata.base_asset_num_queried_sources, 7);
    assert_eq!(exchange_rate.metadata.base_asset_num_received_rates, 7);
    assert_eq!(exchange_rate.metadata.quote_asset_num_queried_sources, 7);
    assert_eq!(exchange_rate.metadata.quote_asset_num_received_rates, 7);
    assert_eq!(exchange_rate.metadata.standard_deviation, 53827575);
    assert_eq!(exchange_rate.rate, 999999986);

    let exchange_rate_2 = scenario_result
        .call_result_2
        .expect("Failed to retrieve an exchange rate 2 from the canister.");
    assert_eq!(exchange_rate.rate, exchange_rate_2.rate);

    println!(
        "r1 = {}, r2 = {}",
        scenario_result.time_passed_1_ms, scenario_result.time_passed_2_ms
    );

    assert!(
        scenario_result.time_passed_1_ms > scenario_result.time_passed_2_ms,
        "r1 = {}, r2 = {}",
        scenario_result.time_passed_1_ms,
        scenario_result.time_passed_2_ms
    );

    assert!(
        scenario_result.time_passed_1_ms / scenario_result.time_passed_2_ms >= 2,
        "Caching should improve response time by two-fold at least"
    );
}
