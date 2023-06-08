use serde_json::json;
use time::format_description;

use crate::container::ResponseBody;

pub fn build_response_body(timestamp: u64) -> ResponseBody {
    let date = time::OffsetDateTime::from_unix_timestamp(timestamp as i64).expect(
        "Failed to make date from given timestamp while build response for Bank of Canada.",
    );
    let format = format_description::parse("[year]-[month]-[day]")
        .expect("Unable to determine time format for Bank of Canada.");
    let date_string = date
        .format(&format)
        .expect("Failed to format date for Bank of Canada.");
    let json = json!({
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
                "d": date_string,
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
    });
    let bytes = serde_json::to_vec(&json).expect("Failed to build Bank of Canada JSON.");
    ResponseBody::Json(bytes)
}
