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
        .responses(Box::new(|exchange| match exchange {
            xrc::Exchange::Coinbase(_) => (
                200,
                Some(json!([
                    [1614596400, 49.15, 60.28, 49.18, 60.19, 12.4941909],
                    [1614596340, 48.01, 49.12, 48.25, 49.08, 19.2031980]
                ])),
            ),
            xrc::Exchange::KuCoin(_) => (200, Some(json!({
                "code":"200000",
                "data":[
                    ["1620296820","345.426","344.396","345.426", "344.096","280.47910557","96614.19641390067"],
                    ["1620296760","344.833","345.468", "345.986","344.832","34.52100408","11916.64690031252"],
                ]
            }))),
        }))
        .run();
}
