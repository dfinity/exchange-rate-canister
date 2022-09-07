use serde_json::json;
use xrc::EXCHANGES;

use crate::container::{run_scenario, Container, ExchangeResponse};

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
            xrc::Exchange::Coinbase(_) => json!([
                [1614596400, 49.15, 60.28, 49.18, 60.19, 12.4941909],
                [1614596340, 48.01, 49.12, 48.25, 49.08, 19.2031980]
            ]),
            xrc::Exchange::KuCoin(_) => json!({
                "code":"200000",
                "data":[
                    ["1614596400","345.426","344.396","345.426", "344.096","280.47910557","96614.19641390067"],
                    ["1614596340","344.833","345.468", "345.986","344.832","34.52100408","11916.64690031252"],
                ]
            }),
            xrc::Exchange::Binance(_) => json!({
                "code":"200000",
                "data":[
                    ["1614596400","345.426","344.396","345.426", "344.096","280.47910557","96614.19641390067"],
                    ["1614596340","344.833","345.468", "345.986","344.832","34.52100408","11916.64690031252"]]
            }),
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
        container.call_canister("get_exchange_rates", (request,))
    });
}
