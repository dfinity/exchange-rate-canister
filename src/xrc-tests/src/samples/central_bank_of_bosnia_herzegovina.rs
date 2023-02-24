use chrono::Utc;
use serde_json::json;

pub fn central_bank_of_bosnia_herzegovina(date: &chrono::DateTime<Utc>) -> Vec<u8> {
    let date = date.format("%Y-%m-%dT00:00:00").to_string();
    serde_json::to_vec(&json!({
        "CurrencyExchangeItems": [
            {
                "Country": "EMU",
                "NumCode": "978",
                "AlphaCode": "EUR",
                "Units": "1",
                "Buy": "1,955830",
                "Middle": "1,955830",
                "Sell": "1,955830",
                "Star": null
            },
            {
                "Country": "China",
                "NumCode": "156",
                "AlphaCode": "CNY",
                "Units": "1",
                "Buy": "0,275802",
                "Middle": "0,276493",
                "Sell": "0,277184",
                "Star": null
            },
            {
                "Country": "Japan",
                "NumCode": "392",
                "AlphaCode": "JPY",
                "Units": "100",
                "Buy": "1,361913",
                "Middle": "1,365326",
                "Sell": "1,368739",
                "Star": null
            },
            {
                "Country": "G,Britain",
                "NumCode": "826",
                "AlphaCode": "GBP",
                "Units": "1",
                "Buy": "2,263272",
                "Middle": "2,268944",
                "Sell": "2,274616",
                "Star": null
            },
            {
                "Country": "USA",
                "NumCode": "840",
                "AlphaCode": "USD",
                "Units": "1",
                "Buy": "1,845384",
                "Middle": "1,850009",
                "Sell": "1,854634",
                "Star": null
            }
        ],
        "Date": date
    }))
    .expect("Failed to serialize")
}
