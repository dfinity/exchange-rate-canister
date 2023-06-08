use serde_json::json;

use crate::container::ResponseBody;

pub fn build_response_body(timestamp: u64) -> ResponseBody {
    let date = time::OffsetDateTime::from_unix_timestamp(timestamp as i64).expect(
        "Failed to make date from given timestamp while build response for Central Bank for Uzbekistan.",
    );
    let format = time::format_description::parse("[day].[month].[year]")
        .expect("Unable to determine time format for Central Bank for Uzbekistan.");
    let date_string = date
        .format(&format)
        .expect("Failed to format date for Central Bank for Uzbekistan.");
    let json = json!([
        {
            "id": 69,
            "Code": "840",
            "Ccy": "USD",
            "CcyNm_RU": "Доллар США",
            "CcyNm_UZ": "AQSH dollari",
            "CcyNm_UZC": "АҚШ доллари",
            "CcyNm_EN": "US Dollar",
            "Nominal": "1",
            "Rate": "10823.52",
            "Diff": "-16.38",
            "Date": &date_string
        },
        {
            "id": 21,
            "Code": "978",
            "Ccy": "EUR",
            "CcyNm_RU": "Евро",
            "CcyNm_UZ": "EVRO",
            "CcyNm_UZC": "EВРО",
            "CcyNm_EN": "Euro",
            "Nominal": "1",
            "Rate": "11439.38",
            "Diff": "0.03",
            "Date": &date_string
        },
        {
            "id": 22,
            "Code": "826",
            "Ccy": "GBP",
            "CcyNm_RU": "Фунт стерлингов",
            "CcyNm_UZ": "Angliya funt sterlingi",
            "CcyNm_UZC": "Англия фунт стерлинги",
            "CcyNm_EN": "Pound Sterling",
            "Nominal": "1",
            "Rate": "13290.20",
            "Diff": "-43.96",
            "Date": &date_string
        },
        {
            "id": 33,
            "Code": "392",
            "Ccy": "JPY",
            "CcyNm_RU": "Иена",
            "CcyNm_UZ": "Yaponiya iyenasi",
            "CcyNm_UZC": "Япония иенаси",
            "CcyNm_EN": "Japan Yen",
            "Nominal": "1",
            "Rate": "80.05",
            "Diff": "-0.23",
            "Date": &date_string
        },
        {
            "id": 15,
            "Code": "156",
            "Ccy": "CNY",
            "CcyNm_RU": "Юань ренминби",
            "CcyNm_UZ": "Xitoy yuani",
            "CcyNm_UZC": "Хитой юани",
            "CcyNm_EN": "Yuan Renminbi",
            "Nominal": "1",
            "Rate": "1617.96",
            "Diff": "-0.93",
            "Date": &date_string
        }
    ]);
    let bytes =
        serde_json::to_vec(&json).expect("Failed to encode the Central Bank of Uzbekistan.");
    ResponseBody::Json(bytes)
}
