use serde_json::json;
use xrc::{
    candid::{Asset, AssetClass, GetExchangeRateRequest, GetExchangeRateResult},
    Exchange, EXCHANGES,
};

use crate::container::{run_scenario, Container, ExchangeResponse};

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

    fn build_response(exchange: &Exchange, asset: &Asset, timestamp: u64) -> ExchangeResponse {
        ExchangeResponse::builder()
            .name(exchange.to_string())
            .url(exchange.get_url(&asset.symbol, &exchange.supported_usd_asset_type().symbol, timestamp))
            .json(match exchange {
                Exchange::Binance(_) => json!([
                    [1614596340000i64,"41.96000000","42.07000000","41.96000000","42.06000000","771.33000000",1637161979999i64,"32396.87850000",63,"504.38000000","21177.00270000","0"]
                ]),
                Exchange::Coinbase(_) => json!([
                    [1614596340, 48.01, 49.12, 48.25, 49.08, 19.2031980]
                ]),
                Exchange::KuCoin(_) => json!({
                    "code":"200000",
                    "data":[
                        ["1614596340","344.833","345.468", "345.986","344.832","34.52100408","11916.64690031252"],
                    ]
                }),
                Exchange::Okx(_) => json!({
                    "code":"0",
                    "msg":"",
                    "data": [
                        ["1614596340000","42.03","42.06","41.96","41.96","319.51605","13432.306077"]
                    ]}),
                Exchange::GateIo(_) => json!([
                    ["1614596340","4659.281408","42.61","42.64","42.55","42.64"]
                ]),
            })
            .build()
    }

    let responses = EXCHANGES
        .iter()
        .flat_map(|exchange| {
            [
                build_response(exchange, &request.base_asset, timestamp),
                build_response(exchange, &request.quote_asset, timestamp),
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
    assert_eq!(exchange_rate.metadata.num_queried_sources, 8);
    assert_eq!(exchange_rate.metadata.num_received_rates, 8);
    assert_eq!(exchange_rate.metadata.standard_deviation_permyriad, 27780);
    assert_eq!(exchange_rate.rate_permyriad, 9973);
}
