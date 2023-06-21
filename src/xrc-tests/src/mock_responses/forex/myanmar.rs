use std::collections::HashMap;

use crate::container::ResponseBody;

pub fn build_response_body(timestamp: u64, rates: HashMap<&str, &str>) -> ResponseBody {
    let json = serde_json::json!({
        "info": "Central Bank of Myanmar",
        "description": "Official Website of Central Bank of Myanmar",
        "timestamp": timestamp,
        "rates": {
            "USD": rates.get("USD").cloned().unwrap_or("1,850.0"),
            "CNY": rates.get("CNY").cloned().unwrap_or("276.72"),
            "JPY": rates.get("JPY").cloned().unwrap_or("1,363.4"),
            "GBP": rates.get("USD").cloned().unwrap_or("2,272.0"),
            "EUR": rates.get("EUR").cloned().unwrap_or("1,959.7"),
        }
    });
    let bytes = serde_json::to_vec(&json).expect("Fail to encode for the Central Bank of Myanmar.");
    ResponseBody::Json(bytes)
}
