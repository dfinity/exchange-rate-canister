use chrono::Utc;

const TEMPLATE: &str = r#"
<?xml version="1.0" encoding="UTF-8"?>
<gesmes:Envelope xmlns:gesmes="http://www.gesmes.org/xml/2002-08-01" xmlns="http://www.ecb.int/vocabulary/2002-08-01/eurofxref">
    <gesmes:subject>Reference rates</gesmes:subject>
    <gesmes:Sender>
        <gesmes:name>European Central Bank</gesmes:name>
    </gesmes:Sender>
    <Cube>
        <Cube time='{{DATE}}'>
            <Cube currency='USD' rate='0.9764' />
            <Cube currency='JPY' rate='141.49' />
            <Cube currency='GBP' rate='0.87070' />
            <Cube currency='CNY' rate='6.9481' />
        </Cube>
    </Cube>
</gesmes:Envelope>
"#;

pub fn central_bank_of_europe(date: &chrono::DateTime<Utc>) -> Vec<u8> {
    TEMPLATE
        .replace("{{DATE}}", &date.format("%Y-%m-%d").to_string())
        .into_bytes()
}
