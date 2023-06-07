use crate::container::ResponseBody;

const TEMPLATE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<gesmes:Envelope xmlns:gesmes="http://www.gesmes.org/xml/2002-08-01" xmlns="http://www.ecb.int/vocabulary/2002-08-01/eurofxref">
    <gesmes:subject>Reference rates</gesmes:subject>
    <gesmes:Sender>
        <gesmes:name>European Central Bank</gesmes:name>
    </gesmes:Sender>
    <Cube>
        <Cube time='[DATE_STRING]'>
            <Cube currency='USD' rate='0.9764' />
            <Cube currency='JPY' rate='141.49' />
            <Cube currency='GBP' rate='0.87070' />
            <Cube currency='CNY' rate='6.9481' />
        </Cube>
    </Cube>
</gesmes:Envelope>"#;

pub fn build_response_body(timestamp: u64) -> ResponseBody {
    let date = time::OffsetDateTime::from_unix_timestamp(timestamp as i64).expect(
        "Failed to make date from given timestamp while build response for European Central Bank.",
    );
    let format = time::format_description::parse("[year]-[month]-[day]")
        .expect("Unable to determine time format for European Central Bank.");
    let date_string = date
        .format(&format)
        .expect("Failed to format date for European Central Bank.");
    let xml = TEMPLATE.replace("[DATE_STRING]", &date_string);
    ResponseBody::Xml(xml.as_bytes().to_vec())
}
