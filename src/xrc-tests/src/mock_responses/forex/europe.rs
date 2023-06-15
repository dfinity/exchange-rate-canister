use std::collections::HashMap;

use crate::container::ResponseBody;

const TEMPLATE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<gesmes:Envelope xmlns:gesmes="http://www.gesmes.org/xml/2002-08-01" xmlns="http://www.ecb.int/vocabulary/2002-08-01/eurofxref">
    <gesmes:subject>Reference rates</gesmes:subject>
    <gesmes:Sender>
        <gesmes:name>European Central Bank</gesmes:name>
    </gesmes:Sender>
    <Cube>
        <Cube time='[DATE_STRING]'>
            <Cube currency='USD' rate='[USD_RATE]' />
            <Cube currency='JPY' rate='[JPY_RATE]' />
            <Cube currency='GBP' rate='[GBP_RATE]' />
            <Cube currency='CNY' rate='[CNY_RATE]' />
        </Cube>
    </Cube>
</gesmes:Envelope>"#;

pub fn build_response_body(timestamp: u64, rates: HashMap<&str, &str>) -> ResponseBody {
    let date = time::OffsetDateTime::from_unix_timestamp(timestamp as i64).expect(
        "Failed to make date from given timestamp while build response for European Central Bank.",
    );
    let format = time::format_description::parse("[year]-[month]-[day]")
        .expect("Unable to determine time format for European Central Bank.");
    let date_string = date
        .format(&format)
        .expect("Failed to format date for European Central Bank.");
    let xml = TEMPLATE
        .replace("[DATE_STRING]", &date_string)
        .replace("[GBP_RATE]", rates.get("gbp").cloned().unwrap_or("0.87070"))
        .replace("[USD_RATE]", rates.get("usd").cloned().unwrap_or("0.9764"))
        .replace("[JPY_RATE]", rates.get("jpy").cloned().unwrap_or("141.49"))
        .replace("[CNY_RATE]", rates.get("cny").cloned().unwrap_or("6.9481"));
    ResponseBody::Xml(xml.as_bytes().to_vec())
}
