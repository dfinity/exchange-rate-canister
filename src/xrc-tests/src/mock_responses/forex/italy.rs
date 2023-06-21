use crate::container::ResponseBody;

pub fn build_response_body(timestamp: u64) -> ResponseBody {
    let date = time::OffsetDateTime::from_unix_timestamp(timestamp as i64)
        .expect("Failed to make date from given timestamp while build response for Bank of Italy.");
    let format = time::format_description::parse("[year]-[month]-[day]")
        .expect("Unable to determine time format for Bank of Italy.");
    let date_string = date
        .format(&format)
        .expect("Failed to format date for Bank for Italy.");
    let json = serde_json::json!({
        "resultsInfo": {
            "totalRecords": 173,
            "timezoneReference": "Le date sono riferite al fuso orario dell'Europa Centrale"
        },
        "rates": [
            {
                "country": "CINA (Repubblica Popolare di)",
                "currency": "Renminbi(Yuan)",
                "isoCode": "CNY",
                "uicCode": "144",
                "avgRate": "7.0775",
                "exchangeConvention": "Quantita' di valuta estera per 1 Euro",
                "exchangeConventionCode": "C",
                "referenceDate": &date_string
            },
            {
                "country": "GIAPPONE",
                "currency": "Yen Giapponese",
                "isoCode": "JPY",
                "uicCode": "071",
                "avgRate": "143.6700",
                "exchangeConvention": "Quantita' di valuta estera per 1 Euro",
                "exchangeConventionCode": "C",
                "referenceDate": &date_string
            },
            {
                "country": "REGNO UNITO",
                "currency": "Sterlina Gran Bretagna",
                "isoCode": "GBP",
                "uicCode": "002",
                "avgRate": "0.86350",
                "exchangeConvention": "Quantita' di valuta estera per 1 Euro",
                "exchangeConventionCode": "C",
                "referenceDate": &date_string
            },
            {
                "country": "STATI UNITI",
                "currency": "Dollaro USA",
                "isoCode": "USD",
                "uicCode": "001",
                "avgRate": "1.0561",
                "exchangeConvention": "Quantita' di valuta estera per 1 Euro",
                "exchangeConventionCode": "C",
                "referenceDate": &date_string
            }
        ]
    });
    let bytes = serde_json::to_vec(&json).expect("Fail to encode for the Bank of Italy.");
    ResponseBody::Json(bytes)
}
