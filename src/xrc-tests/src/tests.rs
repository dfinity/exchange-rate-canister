use serde_json::json;
use xrc::EXCHANGES;

use crate::container::{run_scenario, Container, ExchangeResponse};

#[ignore]
#[test]
fn can_successfully_retrieve_rate() {
    let request = xrc::candid::GetExchangeRateRequest {
        timestamp: Some(1614596340),
        quote_asset: xrc::candid::Asset {
            symbol: "btc".to_string(),
            class: xrc::candid::AssetClass::Cryptocurrency,
        },
        base_asset: xrc::candid::Asset {
            symbol: "icp".to_string(),
            class: xrc::candid::AssetClass::Cryptocurrency,
        },
    };

    let responses = EXCHANGES.iter().map(|exchange| {
        let json = match exchange {
            xrc::Exchange::Binance(_) => json!([
                [1614596340000i64,"41.96000000","42.07000000","41.96000000","42.06000000","771.33000000",1637161979999i64,"32396.87850000",63,"504.38000000","21177.00270000","0"]
            ]),
            xrc::Exchange::Coinbase(_) => json!([
                [1614596340, 48.01, 49.12, 48.25, 49.08, 19.2031980]
            ]),
            xrc::Exchange::KuCoin(_) => json!({
                "code":"200000",
                "data":[
                    ["1614596340","344.833","345.468", "345.986","344.832","34.52100408","11916.64690031252"],
                ]
            }),
            xrc::Exchange::Okx(_) => json!({
                "code":"0",
                "msg":"",
                "data": [
                    ["1614596340000","42.03","42.06","41.96","41.96","319.51605","13432.306077"]
                ]}),
        };

        ExchangeResponse::builder()
            .name(exchange.to_string())
            .url(exchange.get_url(&request.base_asset.symbol, &request.quote_asset.symbol, request.timestamp.unwrap_or_default()))
            .json(json)
            .build()
    }).collect::<Vec<_>>();

    let container = Container::builder()
        .name("can_successfully_retrieve_rate")
        .exchange_responses(responses)
        .build();

    run_scenario(container, |container: &Container| {
        let output = container
            .call_canister::<_, xrc::candid::GetExchangeRateResult>("get_exchange_rate", request)
            .expect("Failed to call canister for rates");

        // Check if the rates found are in the order defined by the `exchanges!` macro call in exchanges.rs:56.
        println!("{:#?}", output);

        Ok(())
    })
    .expect("Scenario failed");
}
