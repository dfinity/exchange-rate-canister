use serde_json::json;

use crate::container::ResponseBody;

pub fn build_response_body(timestamp: u64) -> ResponseBody {
    let date = time::OffsetDateTime::from_unix_timestamp(timestamp as i64).expect(
        "Failed to make date from given timestamp while build response for Central Bank for Bosnia Herzegovina.",
    );
    let format = time::format_description::parse("[year]-[month]-[day]")
        .expect("Unable to determine time format for Central Bank for Bosnia Herzegovina.");
    let date_string = date
        .format(&format)
        .expect("Failed to format date for Central Bank for Bosnia Herzegovina.");
    let json = json!({
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
            }
        ],
        "Date": format!("{}T00:00:00", date_string),
        "Comments": [],
        "Number": 125
    });
    let bytes = serde_json::to_vec(&json)
        .expect("Failed to render bytes for Central Bank for Bosnia Herzegovina.");
    ResponseBody::Json(bytes)
}
