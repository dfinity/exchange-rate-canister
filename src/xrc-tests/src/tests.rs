use ::function_name::named;
use serde_json::json;

use crate::image::Scenario;

#[test]
#[named]
fn can_successfully_retrieve_rate() {
    let _ = Scenario::builder()
        .name(function_name!().to_string())
        .responses(|exchange| match exchange {
            xrc::Exchange::Coinbase(_) => (200, Some(json!({}))),
        })
        .run();
}
