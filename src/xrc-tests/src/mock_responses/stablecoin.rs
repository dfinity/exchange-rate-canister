use serde_json::json;
use xrc::{Exchange, EXCHANGES};

use crate::container::{ExchangeResponse, ResponseBody};

fn sample_stablecoin_json(exchange: &Exchange) -> ResponseBody {
    let json = match exchange {
        Exchange::Coinbase(_) => json!([[1614596340, 1.00, 1.00, 1.01, 1.00, 1.00]]),
        Exchange::KuCoin(_) => json!({
            "code":"200000",
            "data":[
                ["1614596340","0.97","1.00", "1.00","1.00","1.00","1.00"],
            ]
        }),
        Exchange::Okx(_) => json!({
        "code":"0",
        "msg":"",
        "data": [
            ["1614596340000","1.02","1.00","1.00","1.00","1.00","1.00","1.00","1"]
        ]}),
        Exchange::GateIo(_) => json!([[
            "1614596340",
            "1.00",
            "1.00",
            "1.00",
            "1.00",
            "1.00",
            "0",
            "true"
        ]]),
        Exchange::Mexc(_) => json!({
            "code":"200",
            "data": [
                [1664506800,"0.95","1.00","1.00","1.00","1.00","1.00"]
            ]
        }),
        Exchange::Poloniex(_) => json!([[
            "1.00",
            "1.00",
            "0.99",
            "1.00",
            "1.00",
            "1.00",
            "1.00",
            "1.00",
            1,
            1677584374539i64,
            "1.00",
            "MINUTE_1",
            1677584340000i64,
            1677584399999i64
        ]]),
        Exchange::CryptoCom(_) => json!({
            "result": {
                "data": [
                    {
                        "o": "0.99",
                        "h": "1.00",
                        "l": "1.00",
                        "c": "1.00",
                        "v": "0.0000",
                        "t": 1614596340000i64
                    }
                ]
            }
        }),
    };
    ResponseBody::Json(serde_json::to_vec(&json).expect("Failed to encode JSON to bytes"))
}

pub fn build_responses(timestamp: u64) -> impl Iterator<Item = ExchangeResponse> + 'static {
    EXCHANGES.iter().flat_map(move |exchange| {
        exchange
            .supported_stablecoin_pairs()
            .iter()
            .map(move |pair| {
                let url = exchange.get_url(pair.0, pair.1, timestamp);
                ExchangeResponse::builder()
                    .name(exchange.to_string())
                    .url(url)
                    .body(sample_stablecoin_json(exchange))
                    .build()
            })
    })
}
