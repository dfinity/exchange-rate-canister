use crate::container::ResponseBody;

pub fn build_response_body(timestamp: u64) -> ResponseBody {
    let json = serde_json::json!({
        "info": "Central Bank of Myanmar",
        "description": "Official Website of Central Bank of Myanmar",
        "timestamp": timestamp,
        "rates": {
            "USD": "1,850.0",
            "CNY": "276.72",
            "JPY": "1,363.4",
            "GBP": "2,272.0",
            "EUR": "1,959.7"
        }
    });
    let bytes = serde_json::to_vec(&json).expect("Fail to encode for the Central Bank of Myanmar.");
    ResponseBody::Json(bytes)
}
