use std::collections::HashMap;

use crate::container::ResponseBody;

const TEMPLATE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:rba="https://www.rba.gov.au/statistics/frequency/exchange-rates.html" xmlns:cb="http://www.cbwiki.net/wiki/index.php/Specification_1.2/" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns="http://purl.org/rss/1.0/" xsi:schemaLocation="http://www.w3.org/1999/02/22-rdf-syntax-ns# rdf.xsd">
  <item rdf:about="https://www.rba.gov.au/statistics/frequency/exchange-rates.html#USD">
    <cb:statistics rdf:parseType="Resource">
      <cb:exchangeRate rdf:parseType="Resource">
        <rdf:type rdf:resource="http://www.cbwiki.net/wiki/index.php/RSS-CB_1.2_RDF_Schema#Exchange-Rates"/>
        <cb:observation rdf:parseType="Resource">
          <rdf:type rdf:resource="http://www.cbwiki.net/wiki/index.php/RSS-CB_1.2_RDF_Schema#Exchange-Rates"/>
          <cb:value>[USD_RATE]</cb:value>
          <cb:unit>AUD</cb:unit>
          <cb:decimals>4</cb:decimals>
        </cb:observation>
        <cb:targetCurrency>USD</cb:targetCurrency>
        <cb:observationPeriod rdf:parseType="Resource">
          <rdf:type rdf:resource="http://www.cbwiki.net/wiki/index.php/RSS-CB_1.2_RDF_Schema#Exchange-Rates"/>
          <cb:frequency>daily</cb:frequency>
          <cb:period>[DATE_STRING]</cb:period>
        </cb:observationPeriod>
      </cb:exchangeRate>
    </cb:statistics>
  </item>
  <item rdf:about="https://www.rba.gov.au/statistics/frequency/exchange-rates.html#CNY">
    <cb:statistics rdf:parseType="Resource">
      <cb:exchangeRate rdf:parseType="Resource">
        <rdf:type rdf:resource="http://www.cbwiki.net/wiki/index.php/RSS-CB_1.2_RDF_Schema#Exchange-Rates"/>
        <cb:observation rdf:parseType="Resource">
          <rdf:type rdf:resource="http://www.cbwiki.net/wiki/index.php/RSS-CB_1.2_RDF_Schema#Exchange-Rates"/>
          <cb:value>[CNY_RATE]</cb:value>
          <cb:unit>AUD</cb:unit>
          <cb:decimals>4</cb:decimals>
        </cb:observation>
        <cb:targetCurrency>CNY</cb:targetCurrency>
        <cb:observationPeriod rdf:parseType="Resource">
          <rdf:type rdf:resource="http://www.cbwiki.net/wiki/index.php/RSS-CB_1.2_RDF_Schema#Exchange-Rates"/>
          <cb:frequency>daily</cb:frequency>
          <cb:period>[DATE_STRING]</cb:period>
        </cb:observationPeriod>
      </cb:exchangeRate>
    </cb:statistics>
  </item>
  <item rdf:about="https://www.rba.gov.au/statistics/frequency/exchange-rates.html#JPY">
    <cb:statistics rdf:parseType="Resource">
      <cb:exchangeRate rdf:parseType="Resource">
        <cb:observation rdf:parseType="Resource">
          <rdf:type rdf:resource="http://www.cbwiki.net/wiki/index.php/RSS-CB_1.2_RDF_Schema#Exchange-Rates"/>
          <cb:value>[JPY_RATE]</cb:value>
          <cb:unit>AUD</cb:unit>
          <cb:decimals>2</cb:decimals>
        </cb:observation>
        <cb:targetCurrency>JPY</cb:targetCurrency>
        <cb:observationPeriod rdf:parseType="Resource">
          <rdf:type rdf:resource="http://www.cbwiki.net/wiki/index.php/RSS-CB_1.2_RDF_Schema#Exchange-Rates"/>
          <cb:frequency>daily</cb:frequency>
          <cb:period>[DATE_STRING]</cb:period>
        </cb:observationPeriod>
      </cb:exchangeRate>
    </cb:statistics>
  </item>
  <item rdf:about="https://www.rba.gov.au/statistics/frequency/exchange-rates.html#EUR">
    <cb:statistics rdf:parseType="Resource">
      <rdf:type rdf:resource="http://www.cbwiki.net/wiki/index.php/RSS-CB_1.2_RDF_Schema#Exchange-Rates"/>
      <cb:country>AU</cb:country>
      <cb:institutionAbbrev>RBA</cb:institutionAbbrev>
      <cb:exchangeRate rdf:parseType="Resource">
        <rdf:type rdf:resource="http://www.cbwiki.net/wiki/index.php/RSS-CB_1.2_RDF_Schema#Exchange-Rates"/>
        <cb:observation rdf:parseType="Resource">
          <rdf:type rdf:resource="http://www.cbwiki.net/wiki/index.php/RSS-CB_1.2_RDF_Schema#Exchange-Rates"/>
          <cb:value>[EUR_RATE]</cb:value>
          <cb:unit>AUD</cb:unit>
          <cb:decimals>4</cb:decimals>
        </cb:observation>
        <cb:baseCurrency>AUD</cb:baseCurrency>
        <cb:targetCurrency>EUR</cb:targetCurrency>
        <cb:rateType>4.00 pm foreign exchange rates</cb:rateType>
        <cb:observationPeriod rdf:parseType="Resource">
          <rdf:type rdf:resource="http://www.cbwiki.net/wiki/index.php/RSS-CB_1.2_RDF_Schema#Exchange-Rates"/>
          <cb:frequency>daily</cb:frequency>
          <cb:period>[DATE_STRING]</cb:period>
        </cb:observationPeriod>
      </cb:exchangeRate>
    </cb:statistics>
  </item>
  <item rdf:about="https://www.rba.gov.au/statistics/frequency/exchange-rates.html#GBP">
    <cb:statistics rdf:parseType="Resource">
      <rdf:type rdf:resource="http://www.cbwiki.net/wiki/index.php/RSS-CB_1.2_RDF_Schema#Exchange-Rates"/>
      <cb:country>AU</cb:country>
      <cb:institutionAbbrev>RBA</cb:institutionAbbrev>
      <cb:exchangeRate rdf:parseType="Resource">
        <rdf:type rdf:resource="http://www.cbwiki.net/wiki/index.php/RSS-CB_1.2_RDF_Schema#Exchange-Rates"/>
        <cb:observation rdf:parseType="Resource">
          <rdf:type rdf:resource="http://www.cbwiki.net/wiki/index.php/RSS-CB_1.2_RDF_Schema#Exchange-Rates"/>
          <cb:value>[GBP_RATE]</cb:value>
          <cb:unit>AUD</cb:unit>
          <cb:decimals>4</cb:decimals>
        </cb:observation>
        <cb:baseCurrency>AUD</cb:baseCurrency>
        <cb:targetCurrency>GBP</cb:targetCurrency>
        <cb:rateType>4.00 pm foreign exchange rates</cb:rateType>
        <cb:observationPeriod rdf:parseType="Resource">
          <rdf:type rdf:resource="http://www.cbwiki.net/wiki/index.php/RSS-CB_1.2_RDF_Schema#Exchange-Rates"/>
          <cb:frequency>daily</cb:frequency>
          <cb:period>[DATE_STRING]</cb:period>
        </cb:observationPeriod>
      </cb:exchangeRate>
    </cb:statistics>
  </item>
</rdf:RDF>
"#;

pub fn build_response_body(timestamp: u64, rates: HashMap<&str, &str>) -> ResponseBody {
    let date = time::OffsetDateTime::from_unix_timestamp(timestamp as i64).expect(
        "Failed to make date from given timestamp while build response for Reserve Bank of Austrailia.",
    );
    let format = time::format_description::parse("[year]-[month]-[day]")
        .expect("Unable to determine time format for Reserve Bank of Austrailia.");
    let date_string = date
        .format(&format)
        .expect("Failed to format date for Reserve Bank of Austrailia.");
    let xml = TEMPLATE
        .replace("[DATE_STRING]", &date_string)
        .replace("[EUR_RATE]", rates.get("eur").cloned().unwrap_or("0.6128"))
        .replace("[GBP_RATE]", rates.get("gbp").cloned().unwrap_or("0.5377"))
        .replace("[USD_RATE]", rates.get("usd").cloned().unwrap_or("0.6677"))
        .replace("[JPY_RATE]", rates.get("jpy").cloned().unwrap_or("88.98"))
        .replace("[CNY_RATE]", rates.get("cny").cloned().unwrap_or("4.5966"));
    ResponseBody::Xml(xml.as_bytes().to_vec())
}
