use std::time::Instant;

use serde_json::json;
use xrc::{
    candid::{Asset, AssetClass, GetExchangeRateRequest, GetExchangeRateResult},
    usdt_asset, Exchange, EXCHANGES,
};

use crate::container::{run_scenario, Container, ExchangeResponse};

fn get_sample_json_for_exchange(exchange: &Exchange) -> serde_json::Value {
    match exchange {
        Exchange::Binance(_) => json!([[
            1614596340000i64,
            "41.96000000",
            "42.07000000",
            "41.96000000",
            "42.06000000",
            "771.33000000",
            1637161979999i64,
            "32396.87850000",
            63,
            "504.38000000",
            "21177.00270000",
            "0"
        ]]),
        Exchange::Coinbase(_) => json!([[1614596340, 44.01, 45.12, 44.25, 45.08, 19.2031980]]),
        Exchange::KuCoin(_) => json!({
            "code":"200000",
            "data":[
                ["1614596340","44.833","45.468", "45.986","44.832","34.52100408","11916.64690031252"],
            ]
        }),
        Exchange::Okx(_) => json!({
        "code":"0",
        "msg":"",
        "data": [
            ["1614596340000","42.03","42.06","41.96","41.96","319.51605","13432.306077"]
        ]}),
        Exchange::GateIo(_) => json!([[
            "1614596340",
            "4659.281408",
            "42.61",
            "42.64",
            "42.55",
            "42.64"
        ]]),
        Exchange::Mexc(_) => json!({
            "code":"200",
            "data": [
                [1664506800,"46.101","46.105","46.107","46.101","45.72","34.928"]
            ]
        }),
    }
}

fn build_response(
    exchange: &Exchange,
    asset: &Asset,
    timestamp: u64,
    json: serde_json::Value,
) -> ExchangeResponse {
    ExchangeResponse::builder()
        .name(exchange.to_string())
        .url(exchange.get_url(&asset.symbol, &usdt_asset().symbol, timestamp))
        .json(json)
        .build()
}

/// This test is used to confirm that the exchange rate canister can receive
/// a request to the `get_exchange_rate` endpoint and successfully return a
/// computed rate for the provided assets.
#[ignore]
#[test]
fn can_successfully_retrieve_rate() {
    let timestamp = 1614596340;
    let request = GetExchangeRateRequest {
        timestamp: Some(timestamp),
        quote_asset: Asset {
            symbol: "btc".to_string(),
            class: AssetClass::Cryptocurrency,
        },
        base_asset: Asset {
            symbol: "icp".to_string(),
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
        .name("can_successfully_retrieve_rate")
        .exchange_responses(responses)
        .build();

    let request_ = request.clone();
    let exchange_rate_result = run_scenario(container, |container: &Container| {
        Ok(container
            .call_canister::<_, GetExchangeRateResult>("get_exchange_rate", request_)
            .expect("Failed to call canister for rates"))
    })
    .expect("Scenario failed");

    let exchange_rate =
        exchange_rate_result.expect("Failed to retrieve an exchange rate from the canister.");
    assert_eq!(exchange_rate.base_asset, request.base_asset);
    assert_eq!(exchange_rate.quote_asset, request.quote_asset);
    assert_eq!(exchange_rate.timestamp, timestamp);
    assert_eq!(exchange_rate.metadata.base_asset_num_queried_sources, 6);
    assert_eq!(exchange_rate.metadata.base_asset_num_received_rates, 6);
    assert_eq!(exchange_rate.metadata.quote_asset_num_queried_sources, 6);
    assert_eq!(exchange_rate.metadata.quote_asset_num_received_rates, 6);
    assert_eq!(exchange_rate.metadata.standard_deviation, 54427089);
    assert_eq!(exchange_rate.rate, 999999979);
}

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
            symbol: "btc".to_string(),
            class: AssetClass::Cryptocurrency,
        },
        base_asset: Asset {
            symbol: "icp".to_string(),
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
    assert_eq!(exchange_rate.metadata.base_asset_num_queried_sources, 6);
    assert_eq!(exchange_rate.metadata.base_asset_num_received_rates, 6);
    assert_eq!(exchange_rate.metadata.quote_asset_num_queried_sources, 6);
    assert_eq!(exchange_rate.metadata.quote_asset_num_received_rates, 6);
    assert_eq!(exchange_rate.metadata.standard_deviation, 50499737);
    assert_eq!(exchange_rate.rate, 999999979);

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
