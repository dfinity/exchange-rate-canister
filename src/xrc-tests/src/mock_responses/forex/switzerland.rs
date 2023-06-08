use time::format_description;

use crate::container::ResponseBody;

const TEMPLATE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<wechselkurse xmlns="https://www.backend-rates.ezv.admin.ch/xmldaily" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:schemaLocation="https://www.backend-rates.ezv.admin.ch/xmldaily https://www.backend-rates.ezv.admin.ch/dailyrates.xsd">
  <datum>[DATE_STRING]</datum>
  <zeit>02:00:01</zeit>
  <gueltigkeit>29.06.2022</gueltigkeit>
  <devise code="eur">
    <land_de>Europäische Währungsunion</land_de>
    <land_fr>Union monétaire européenne</land_fr>
    <land_it>Unione Monetaria Europea</land_it>
    <land_en>Euro Member</land_en>
    <waehrung>1 EUR</waehrung>
    <kurs>1.02111</kurs>
  </devise>
  <devise code="usd">
    <land_de>USA</land_de>
    <land_fr>USA</land_fr>
    <land_it>USA</land_it>
    <land_en>United States</land_en>
    <waehrung>1 USD</waehrung>
    <kurs>0.96566</kurs>
  </devise>
  <devise code="cny">
    <land_de>China</land_de>
    <land_fr>Chine</land_fr>
    <land_it>Cina</land_it>
    <land_en>China Yuan</land_en>
    <waehrung>100 CNY</waehrung>
    <kurs>14.41306</kurs>
  </devise>
  <devise code="gbp">
    <land_de>Grossbritannien</land_de>
    <land_fr>Grande-Bretagne</land_fr>
    <land_it>Gran Bretagna</land_it>
    <land_en>United Kingdom</land_en>
    <waehrung>1 GBP</waehrung>
    <kurs>1.18448</kurs>
  </devise>
  <devise code="jpy">
    <land_de>Japan</land_de>
    <land_fr>Japon</land_fr>
    <land_it>Giappone</land_it>
    <land_en>Japan</land_en>
    <waehrung>100 JPY</waehrung>
    <kurs>0.71455</kurs>
  </devise>
</wechselkurse>
"#;

pub fn build_response_body(timestamp: u64) -> ResponseBody {
    let date = time::OffsetDateTime::from_unix_timestamp(timestamp as i64).expect(
        "Failed to make date from given timestamp while build response for Swiss Office for Customs.",
    );
    let format = format_description::parse("[day].[month].[year]")
        .expect("Unable to determine time format for Swiss Office for Customs.");
    let date_string = date
        .format(&format)
        .expect("Failed to format date for Swiss Office for Customs.");
    let xml = TEMPLATE.replace("[DATE_STRING]", &date_string);
    ResponseBody::Xml(xml.as_bytes().to_vec())
}
