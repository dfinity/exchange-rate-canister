use serde_json::json;

use crate::container::ResponseBody;

pub fn build_response_body(timestamp: u64) -> ResponseBody {
    let date = time::OffsetDateTime::from_unix_timestamp(timestamp as i64).expect(
        "Failed to make date from given timestamp while build response for Central Bank for Nepal.",
    );
    let format = time::format_description::parse("[year]-[month]-[day]")
        .expect("Unable to determine time format for Central Bank for Nepal.");
    let date_string = date
        .format(&format)
        .expect("Failed to format date for Central Bank for Nepal.");
    let json = json!({
        "status": {
            "code": 200
        },
        "errors": {
            "validation": null
        },
        "params": {
            "date": null,
            "from": "2022-06-28",
            "to": "2022-06-28",
            "post_type": null,
            "per_page": "100",
            "page": "1",
            "slug": null,
            "q": null
        },
        "data": {
            "payload": [
                {
                    "date": date_string,
                    "published_on": "2022-06-28 00:00:13",
                    "modified_on": "2022-06-27 19:58:47",
                    "rates": [
                        {
                            "currency": {
                                "iso3": "USD",
                                "name": "U.S. Dollar",
                                "unit": 1
                            },
                            "buy": "125.05",
                            "sell": "125.65"
                        },
                        {
                            "currency": {
                                "iso3": "EUR",
                                "name": "European Euro",
                                "unit": 1
                            },
                            "buy": "132.37",
                            "sell": "133.00"
                        },
                        {
                            "currency": {
                                "iso3": "GBP",
                                "name": "UK Pound Sterling",
                                "unit": 1
                            },
                            "buy": "153.56",
                            "sell": "154.30"
                        },
                        {
                            "currency": {
                                "iso3": "JPY",
                                "name": "Japanese Yen",
                                "unit": 10
                            },
                            "buy": "9.25",
                            "sell": "9.29"
                        },
                        {
                            "currency": {
                                "iso3": "CNY",
                                "name": "Chinese Yuan",
                                "unit": 1
                            },
                            "buy": "18.69",
                            "sell": "18.78"
                        }
                    ]
                }
            ]
        },
        "pagination": {
            "page": 1,
            "pages": 1,
            "per_page": 100,
            "total": 1,
            "links": {
                "prev": null,
                "next": null
            }
        }
    });
    let bytes = serde_json::to_vec(&json).expect("Fail to encode for the Central Bank of Nepal.");
    ResponseBody::Json(bytes)
}
