use std::time::Instant;

use ic_xrc_types::{Asset, AssetClass, GetExchangeRateRequest, GetExchangeRateResult};

use crate::{
    container::{run_scenario, Container},
    mock_responses,
};

/// Setup:
/// * Deploy mock FOREX data providers and exchanges with a large response delay
///
/// Start replicas and deploy the XRC, configured to use the mock data sources
///
/// Runbook:
///
/// 1. Request the same exchange rate many times per second for a bounded time, e.g., 1 minute
/// 2. Assert that all requests are answered with the same rate with a small delay after the first response (due to caching in the XRC)
///
/// Success criteria:
///
/// * All queries are handled correctly
#[ignore]
#[test]
fn caching() {
    struct NewScenarioResult {
        result: GetExchangeRateResult,
        time_passed_millis: u128,
    }

    let mut samples: Vec<NewScenarioResult> = vec![];
    let now = time::OffsetDateTime::now_utc().unix_timestamp() as u64;
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

    let responses = mock_responses::exchanges::build_common_responses(
        request.base_asset.symbol.clone(),
        timestamp,
    )
    .chain(mock_responses::exchanges::build_common_responses(
        request.quote_asset.symbol.clone(),
        timestamp,
    ))
    .chain(mock_responses::stablecoin::build_responses(timestamp))
    .chain(mock_responses::forex::build_responses(now))
    .collect::<Vec<_>>();
    let container = Container::builder()
        .name("caching")
        .exchange_responses(responses)
        .build();

    let run_scenario_instant = Instant::now();
    run_scenario(container, |container| {
        let initial_call = container
            .call_canister::<_, GetExchangeRateResult>("get_exchange_rate", &request)
            .expect("Failed to call canister for rates");

        while run_scenario_instant.elapsed().as_secs() >= 60 {
            let iteration_instant = Instant::now();
            let result = container
                .call_canister::<_, GetExchangeRateResult>("get_exchange_rate", &request)
                .expect("Failed to call canister for rates");

            samples.push(NewScenarioResult {
                result,
                time_passed_millis: iteration_instant.elapsed().as_millis(),
            })
        }

        Ok(())
    })
    .expect("Scenario failed");

    struct ScenarioResult {
        call_result_1: GetExchangeRateResult,
        time_passed_1_ms: u128,
        call_result_2: GetExchangeRateResult,
        time_passed_2_ms: u128,
    }

    let now = time::OffsetDateTime::now_utc().unix_timestamp() as u64;
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

    let scenario_result = run_scenario(container, |container: &Container| {
        let instant = Instant::now();
        let call_result_1 = container
            .call_canister::<_, GetExchangeRateResult>("get_exchange_rate", &request)
            .expect("Failed to call canister for rates");
        let time_passed_1_ms = instant.elapsed().as_millis();

        let instant = Instant::now();
        let call_result_2 = container
            .call_canister::<_, GetExchangeRateResult>("get_exchange_rate", &request)
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
    assert_eq!(exchange_rate.rate, 999999980);

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
