use std::collections::HashMap;

use crate::container::ResponseBody;

const TEMPLATE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<?xml-stylesheet type="text/xsl" href="isokur.xsl"?>
<Tarih_Date Tarih="[DATE_STRING]" Date="01/31/2024"  Bulten_No="2024/22" >
	<Currency CrossOrder="0" Kod="USD" CurrencyCode="USD">
		<Unit>1</Unit>
		<Isim>ABD DOLARI</Isim>
		<CurrencyName>US DOLLAR</CurrencyName>
		<ForexBuying>[USD_BUY]</ForexBuying>
		<ForexSelling>[USD_SELL]</ForexSelling>
		<BanknoteBuying>30.2740</BanknoteBuying>
		<BanknoteSelling>30.3953</BanknoteSelling>
		<CrossRateUSD/>
		<CrossRateOther/>
	</Currency>
	<Currency CrossOrder="1" Kod="AUD" CurrencyCode="AUD">
		<Unit>1</Unit>
		<Isim>AVUSTRALYA DOLARI</Isim>
		<CurrencyName>AUSTRALIAN DOLLAR</CurrencyName>
		<ForexBuying>[AUD_BUY]</ForexBuying>
		<ForexSelling>[AUD_SELL]</ForexSelling>
		<BanknoteBuying>19.7866</BanknoteBuying>
		<BanknoteSelling>20.1277</BanknoteSelling>
		<CrossRateUSD>1.5205</CrossRateUSD>
		<CrossRateOther/>
	</Currency>
	<Currency CrossOrder="2" Kod="DKK" CurrencyCode="DKK">
		<Unit>1</Unit>
		<Isim>DANİMARKA KRONU</Isim>
		<CurrencyName>DANISH KRONE</CurrencyName>
		<ForexBuying>[DKK_BUY]</ForexBuying>
		<ForexSelling>[DKK_SELL]</ForexSelling>
		<BanknoteBuying>4.3883</BanknoteBuying>
		<BanknoteSelling>4.4231</BanknoteSelling>
		<CrossRateUSD>6.8881</CrossRateUSD>
		<CrossRateOther/>
	</Currency>
	<Currency CrossOrder="9" Kod="EUR" CurrencyCode="EUR">
		<Unit>1</Unit>
		<Isim>EURO</Isim>
		<CurrencyName>EURO</CurrencyName>
		<ForexBuying>[EUR_BUY]</ForexBuying>
		<ForexSelling>[EUR_SELL]</ForexSelling>
		<BanknoteBuying>32.7655</BanknoteBuying>
		<BanknoteSelling>32.8968</BanknoteSelling>
		<CrossRateUSD/>
		<CrossRateOther>1.0823</CrossRateOther>
	</Currency>
	<Currency CrossOrder="10" Kod="GBP" CurrencyCode="GBP">
		<Unit>1</Unit>
		<Isim>İNGİLİZ STERLİNİ</Isim>
		<CurrencyName>POUND STERLING</CurrencyName>
		<ForexBuying>[GBP_BUY]</ForexBuying>
		<ForexSelling>[GBP_SELL]</ForexSelling>
		<BanknoteBuying>38.3130</BanknoteBuying>
		<BanknoteSelling>38.5975</BanknoteSelling>
		<CrossRateUSD/>
		<CrossRateOther>1.2677</CrossRateOther>
	</Currency>
	<Currency CrossOrder="5" Kod="JPY" CurrencyCode="JPY">
		<Unit>100</Unit>
		<Isim>JAPON YENİ</Isim>
		<CurrencyName>JAPENESE YEN</CurrencyName>
		<ForexBuying>[JPY_BUY]</ForexBuying>
		<ForexSelling>[JPY_SELL]</ForexSelling>
		<BanknoteBuying>20.3812</BanknoteBuying>
		<BanknoteSelling>20.6706</BanknoteSelling>
		<CrossRateUSD>147.74</CrossRateUSD>
		<CrossRateOther/>
	</Currency>
</Tarih_Date>
"#;

pub fn build_response_body(timestamp: u64, rates: HashMap<&str, &str>) -> ResponseBody {
    let date = time::OffsetDateTime::from_unix_timestamp(timestamp as i64).expect(
        "Failed to make date from given timestamp while build response for the Central Bank of Turkey.",
    );
    let format = time::format_description::parse("[day].[month].[year]")
        .expect("Unable to determine time format for the Central Bank of Turkey.");
    let date_string = date
        .format(&format)
        .expect("Failed to format date for the Central Bank of Turkey.");
    let xml = TEMPLATE
        .replace("[DATE_STRING]", &date_string)
        .replace("[EUR_BUY]", rates.get("EUR").cloned().unwrap_or("32.7884"))
        .replace("[EUR_SELL]", rates.get("EUR").cloned().unwrap_or("32.8475"))
        .replace("[GBP_BUY]", rates.get("GBP").cloned().unwrap_or("38.3398"))
        .replace("[GBP_SELL]", rates.get("GBP").cloned().unwrap_or("38.5397"))
        .replace("[USD_BUY]", rates.get("USD").cloned().unwrap_or("30.2952"))
        .replace("[USD_SELL]", rates.get("USD").cloned().unwrap_or("30.3497"))
        .replace("[JPY_BUY]", rates.get("JPY").cloned().unwrap_or("20.4569"))
        .replace("[JPY_SELL]", rates.get("JPY").cloned().unwrap_or("20.5924"))
        .replace("[CNY_BUY]", rates.get("DKK").cloned().unwrap_or("4.3914"))
        .replace("[CNY_SELL]", rates.get("DKK").cloned().unwrap_or("4.4129"))
        .replace("[AUD_BUY]", rates.get("AUD").cloned().unwrap_or("19.8780"))
        .replace("[AUD_SELL]", rates.get("AUD").cloned().unwrap_or("20.0076"));
    ResponseBody::Xml(xml.as_bytes().to_vec())
}
