mod can_successfully_cache_rates;
mod can_successfully_retrieve_rate;

use chrono::Utc;
use serde_json::json;
use xrc::{candid::Asset, usdt_asset, Exchange, Forex, FOREX_SOURCES};

use crate::container::{ExchangeResponse, ResponseBody};

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
            ["1614596340000","42.03","42.06","41.96","41.96","319.51605","13432.306077","13432.306077","1"]
        ]}),
        Exchange::GateIo(_) => json!([[
            "1614596340",
            "4659.281408",
            "42.61",
            "42.64",
            "42.55",
            "42.64",
            "0"
        ]]),
        Exchange::Mexc(_) => json!({
            "code":"200",
            "data": [
                [1664506800,"46.101","46.105","46.107","46.101","45.72","34.928"]
            ]
        }),
    }
}

fn get_forex_sample(forex: &Forex, date: &chrono::DateTime<Utc>) -> ResponseBody {
    match forex {
        Forex::MonetaryAuthorityOfSingapore(_) => ResponseBody::Json(_),
        Forex::CentralBankOfMyanmar(_) => {
            ResponseBody::Json(crate::samples::central_bank_of_myanmar(&date))
        }
        Forex::CentralBankOfBosniaHerzegovina(_) => ResponseBody::Json(_),
        Forex::BankOfIsrael(_) => ResponseBody::Xml(crate::samples::bank_of_israel(&date)),
        Forex::EuropeanCentralBank(_) => {
            ResponseBody::Xml(crate::samples::central_bank_of_europe(&date))
        }
        Forex::BankOfCanada(_) => ResponseBody::Json(crate::samples::bank_of_canada(&date)),
        Forex::CentralBankOfUzbekistan(_) => ResponseBody::Json(_),
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

fn build_forex_response(forex: &Forex, body: ResponseBody, timestamp: u64) -> ExchangeResponse {
    ExchangeResponse::builder()
        .name(forex.to_string())
        .url(forex.get_url(timestamp))
        .body(body)
        .build()
}

fn build_exchange_responses() {}

const ONE_DAY: u64 = 60 * 60 * 24;

/// This function generates all of the responses for the forex sources.
fn build_forex_responses(current_datetime: chrono::DateTime<Utc>) -> Vec<ExchangeResponse> {
    FOREX_SOURCES
        .iter()
        .flat_map(|forex| {
            // Generate responses for forexes from tomorrow and today.
            // This is to eliminate possible issues around time drift from response generation
            // actually running the tests.
            (-1..1)
                .map(|days| {
                    let datetime = current_datetime - chrono::Duration::days(days);
                    let body = get_forex_sample(forex, &datetime);
                    build_forex_response(forex, body, datetime.timestamp() as u64)
                })
                .collect::<Vec<ExchangeResponse>>()
        })
        .collect()
}
