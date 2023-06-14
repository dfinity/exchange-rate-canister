use serde_json::json;
use xrc::{usdt_asset, Exchange, EXCHANGES};

use crate::container::{ExchangeResponse, ResponseBody};

/// Build the responses for cryptocurrency exchanges by providing the base and quote asset symbols, the timestamp, and a rate lookup function.
/// The rate lookup function expects to return rates in a string format (ex. 1.00) per exchange.
/// If the rate function returns None for an exchange, an empty response is created (useful for simulating exchange failure).
pub fn build_responses<F>(
    asset_symbol: String,
    timestamp: u64,
    rate_lookup: F,
) -> impl Iterator<Item = ExchangeResponse> + 'static
where
    F: Fn(&Exchange) -> Option<&str> + 'static,
{
    EXCHANGES.iter().map(move |exchange| {
        let url = exchange.get_url(&asset_symbol, &usdt_asset().symbol, timestamp);
        let body = rate_lookup(exchange)
            .map(move |rate| {
                let json = match exchange {
                    Exchange::Binance(_) => json!([[
                        (timestamp * 1_000) as i64,
                        rate,
                        "1.00",
                        "1.00",
                        "1.00",
                        "1.00",
                        1637161979999i64,
                        "1.00",
                        63,
                        "1.00",
                        "1.00",
                        "0"
                    ]]),
                    Exchange::Coinbase(_) => {
                        let parsed_rate = rate
                            .parse::<f32>()
                            .expect("Failed to parse rate for coinbase");
                        json!([[timestamp, 1.00, 1.00, parsed_rate, 1.00, 1.00]])
                    }
                    Exchange::KuCoin(_) => json!({
                        "code":"200000",
                        "data":[
                            [timestamp.to_string(), rate,"1.00", "1.00","1.00","1.00","1.00"],
                        ]
                    }),
                    Exchange::Okx(_) => {
                        json!({
                            "code":"0",
                            "msg":"",
                            "data": [
                                [(timestamp * 1000).to_string(), rate,"1.00","1.00","1.00","1.00","1.00","1.00","1"]
                            ]
                        })
                    },
                    Exchange::GateIo(_) => json!([[timestamp.to_string(), "1.00", "1.00", rate, "1.00", "1.00", "0"]]),
                    Exchange::Mexc(_) => json!({
                        "code":"200",
                        "data": [
                            [timestamp, rate, "1.00", "1.00", "1.00", "1.00", "1.00"]
                        ]
                    }),
                    Exchange::Poloniex(_) =>json!([[
                        "1.00",
                        "1.00",
                        rate,
                        "1.00",
                        "1.00",
                        "1.00",
                        "1.00",
                        "1.00",
                        1,
                        1677584374539i64,
                        "1.00",
                        "MINUTE_1",
                        (timestamp * 1_000) as i64,
                        1677584399999i64
                    ]]),
                };
                let bytes = serde_json::to_vec(&json).expect("Failed to build exchange response.");
                ResponseBody::Json(bytes)
            })
            .unwrap_or_default();
        ExchangeResponse::builder()
            .name(exchange.to_string())
            .url(url)
            .body(body)
            .build()
    })
}
