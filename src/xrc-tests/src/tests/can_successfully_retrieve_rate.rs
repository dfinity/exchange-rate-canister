use std::time::{SystemTime, UNIX_EPOCH};

use xrc::{
    candid::{Asset, AssetClass, GetExchangeRateRequest, GetExchangeRateResult},
    EXCHANGES, FOREX_SOURCES,
};

use crate::{
    container::{run_scenario, Container},
    tests::{
        build_exchange_response, build_forex_response, get_sample_body_for_forex,
        get_sample_json_for_exchange,
    },
};

/// This test is used to confirm that the exchange rate canister can receive
/// a request to the `get_exchange_rate` endpoint and successfully return a
/// computed rate for the provided assets.
#[ignore]
#[test]
fn can_successfully_retrieve_rate() {
    let one_day = 24 * 60 * 60;
    let yesterday = (UNIX_EPOCH.elapsed().expect("").as_secs() / one_day * one_day) - (one_day);
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

    let mut responses = EXCHANGES
        .iter()
        .flat_map(|exchange| {
            let json = get_sample_json_for_exchange(exchange);
            [
                build_exchange_response(exchange, &request.base_asset, timestamp, json.clone()),
                build_exchange_response(exchange, &request.quote_asset, timestamp, json),
            ]
        })
        .collect::<Vec<_>>();

    let forex = FOREX_SOURCES.get(1).unwrap();
    responses.push(build_forex_response(
        forex,
        yesterday,
        get_sample_body_for_forex(forex, yesterday),
    ));

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
    assert_eq!(exchange_rate.metadata.standard_deviation, 50499737);
    assert_eq!(exchange_rate.rate, 999999986);
}
