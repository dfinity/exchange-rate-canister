use std::collections::HashMap;

use crate::container::ResponseBody;

pub fn build_response_body(timestamp: u64, rates: HashMap<&str, &str>) -> ResponseBody {
    let date = time::OffsetDateTime::from_unix_timestamp(timestamp as i64).expect(
        "Failed to make date from given timestamp while build response for Monetary Authority of Singapore.",
    );
    let format = time::format_description::parse("[year]-[month]-[day]")
        .expect("Unable to determine time format for Monetary Authority of Singapore.");
    let date_string = date
        .format(&format)
        .expect("Failed to format date for Monetary Authority of Singapore.");

    let json = serde_json::json!({
        "success": true,
        "result": {
            "resource_id": [
                "95932927-c8bc-4e7a-b484-68a66a24edfe"
            ],
            "limit": 10,
            "total": "1",
            "records": [
                {
                    "end_of_day": date_string,
                    "preliminary": "0",
                    "eur_sgd": rates.get("EUR").cloned().unwrap_or("1.4661"),
                    "gbp_sgd": rates.get("GBP").cloned().unwrap_or("1.7007"),
                    "usd_sgd": rates.get("USD").cloned().unwrap_or("1.3855"),
                    "cny_sgd_100": rates.get("CNY").cloned().unwrap_or("20.69"),
                    "jpy_sgd_100": rates.get("JPY").cloned().unwrap_or("1.0239"),
                    "timestamp": "1663273633"
                }
            ]
        }
    });
    let bytes =
        serde_json::to_vec(&json).expect("Fail to encode for the Monetary Authority of Singapore.");
    ResponseBody::Json(bytes)
}
