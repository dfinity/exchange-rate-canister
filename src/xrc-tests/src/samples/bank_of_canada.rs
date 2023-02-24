use chrono::Utc;
use serde_json::json;

pub fn bank_of_canada(date: &chrono::DateTime<Utc>) -> Vec<u8> {
    let date = date.format("%Y-%m-%d").to_string();
    serde_json::to_vec(&json!({
        "seriesDetail": {
            "FXCNYCAD": {
                "label": "CNY/CAD",
                "description": "Chinese renminbi to Canadian dollar daily exchange rate",
                "dimension": {
                    "key": "d",
                    "name": "date"
                }
            },
            "FXEURCAD": {
                "label": "EUR/CAD",
                "description": "European euro to Canadian dollar daily exchange rate",
                "dimension": {
                    "key": "d",
                    "name": "date"
                }
            },
            "FXJPYCAD": {
                "label": "JPY/CAD",
                "description": "Japanese yen to Canadian dollar daily exchange rate",
                "dimension": {
                    "key": "d",
                    "name": "date"
                }
            },
            "FXGBPCAD": {
                "label": "GBP/CAD",
                "description": "UK pound sterling to Canadian dollar daily exchange rate",
                "dimension": {
                    "key": "d",
                    "name": "date"
                }
            },
            "FXUSDCAD": {
                "label": "USD/CAD",
                "description": "US dollar to Canadian dollar daily exchange rate",
                "dimension": {
                    "key": "d",
                    "name": "date"
                }
            }
        },
        "observations": [
            {
                "d": date,
                "FXCNYCAD": {
                    "v": "0.1918"
                },
                "FXEURCAD": {
                    "v": "1.3545"
                },
                "FXJPYCAD": {
                    "v": "0.009450"
                },
                "FXGBPCAD": {
                    "v": "1.5696"
                },
                "FXUSDCAD": {
                    "v": "1.2864"
                }
            }
        ]
    }))
    .expect("Failed to serialize")
}
