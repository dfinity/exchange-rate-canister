use chrono::Utc;
use serde_json::json;

pub fn central_bank_of_myanmar(date: &chrono::DateTime<Utc>) -> Vec<u8> {
    let timestamp = date.timestamp();
    serde_json::to_vec(&json!({
        "timestamp": timestamp,
        "rates": {
            "USD": "1,850.0",
            "CNY": "276.72",
            "JPY": "1,363.4",
            "GBP": "2,272.0",
            "EUR": "1,959.7"
        }
    }))
    .expect("Failed to serialize")
}
