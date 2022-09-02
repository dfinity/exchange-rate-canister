use ::function_name::named;
use serde_json::json;

use crate::image::Scenario;

#[test]
#[named]
fn can_successfully_retrieve_rate() {
    let _ = Scenario::builder()
        .name(function_name!().to_string())
        .request(xrc::candid::GetExchangeRateRequest {
            timestamp: Some(1614596340),
            quote_asset: xrc::candid::Asset {
                symbol: "btc".to_string(),
                class: xrc::candid::AssetClass::Cryptocurrency,
            },
            base_asset: xrc::candid::Asset {
                symbol: "icp".to_string(),
                class: xrc::candid::AssetClass::Cryptocurrency,
            },
        })
        .responses(|exchange| match exchange {
            xrc::Exchange::Coinbase(_) => (
                200,
                Some(json!([
                    [1614596400, 49.15, 60.28, 49.18, 60.19, 12.4941909],
                    [1614596340, 48.01, 49.12, 48.25, 49.08, 19.2031980]
                ])),
            ),
        })
        .run();
}
