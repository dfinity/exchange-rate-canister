use serde_json::json;

use crate::container::ResponseBody;

pub fn build_response_body(timestamp: u64) -> ResponseBody {
    let date = time::OffsetDateTime::from_unix_timestamp(timestamp as i64).expect(
        "Failed to make date from given timestamp while build response for Central Bank of Georgia.",
    );
    let format = time::format_description::parse("[year]-[month]-[day]")
        .expect("Unable to determine time format for Central Bank of Georgia.");
    let date_string = date
        .format(&format)
        .expect("Failed to format date for Central Bank of Georgia.");
    let json = json!([
        {
            "date": format!("{}T00:00:00.000Z", date_string),
            "currencies": [
                {
                    "code": "CNY",
                    "quantity": 10,
                    "rateFormated": "4.3877",
                    "diffFormated": "0.0027",
                    "rate": 4.3877,
                    "name": "China Renminbi",
                    "diff": 0.0027,
                    "date": "2022-06-27T17:45:13.527Z",
                    "validFromDate": "2022-06-28T00:00:00.000Z"
                },
                {
                    "code": "EUR",
                    "quantity": 1,
                    "rateFormated": "3.1066",
                    "diffFormated": "0.0112",
                    "rate": 3.1066,
                    "name": "Euro",
                    "diff": 0.0112,
                    "date": "2022-06-27T17:45:13.527Z",
                    "validFromDate": "2022-06-28T00:00:00.000Z"
                },
                {
                    "code": "GBP",
                    "quantity": 1,
                    "rateFormated": "3.6049",
                    "diffFormated": "0.0073",
                    "rate": 3.6049,
                    "name": "United Kingdom Pound",
                    "diff": -0.0073,
                    "date": "2022-06-27T17:45:13.527Z",
                    "validFromDate": "2022-06-28T00:00:00.000Z"
                },
                {
                    "code": "JPY",
                    "quantity": 100,
                    "rateFormated": "2.1706",
                    "diffFormated": "0.0031",
                    "rate": 2.1706,
                    "name": "Japanese Yen",
                    "diff": -0.0031,
                    "date": "2022-06-27T17:45:13.527Z",
                    "validFromDate": "2022-06-28T00:00:00.000Z"
                },
                {
                    "code": "USD",
                    "quantity": 1,
                    "rateFormated": "2.9349",
                    "diffFormated": "0.0011",
                    "rate": 2.9349,
                    "name": "US Dollar",
                    "diff": -0.0011,
                    "date": "2022-06-27T17:45:13.527Z",
                    "validFromDate": "2022-06-28T00:00:00.000Z"
                }
            ]
        }
    ]);
    let bytes = serde_json::to_vec(&json).expect("Fail to encode for the Central Bank of Georgia.");
    ResponseBody::Json(bytes)
}
