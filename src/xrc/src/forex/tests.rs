use maplit::hashmap;

use crate::candid::ExchangeRate;
use crate::utils::test::load_file;
use crate::{ExchangeRateMetadata, DECIMALS};

use super::*;

/// The function test if the macro correctly generates the
/// [core::fmt::Display] trait's implementation for [Forex].
#[test]
fn forex_to_string_returns_name() {
    let forex = Forex::MonetaryAuthorityOfSingapore(MonetaryAuthorityOfSingapore);
    assert_eq!(forex.to_string(), "MonetaryAuthorityOfSingapore");
    let forex = Forex::CentralBankOfMyanmar(CentralBankOfMyanmar);
    assert_eq!(forex.to_string(), "CentralBankOfMyanmar");
    let forex = Forex::CentralBankOfBosniaHerzegovina(CentralBankOfBosniaHerzegovina);
    assert_eq!(forex.to_string(), "CentralBankOfBosniaHerzegovina");
    let forex = Forex::BankOfIsrael(BankOfIsrael);
    assert_eq!(forex.to_string(), "BankOfIsrael");
    let forex = Forex::EuropeanCentralBank(EuropeanCentralBank);
    assert_eq!(forex.to_string(), "EuropeanCentralBank");
    let forex = Forex::BankOfCanada(BankOfCanada);
    assert_eq!(forex.to_string(), "BankOfCanada");
    let forex = Forex::CentralBankOfUzbekistan(CentralBankOfUzbekistan);
    assert_eq!(forex.to_string(), "CentralBankOfUzbekistan");
}

/// The function tests if the macro correctly generates derive copies by
/// verifying that the forex sources return the correct query string.
#[test]
fn query_string() {
    // Note that the hours/minutes/seconds are ignored, setting the considered timestamp to 1661472000.
    let timestamp = 1661524016;
    let singapore = MonetaryAuthorityOfSingapore;
    let query_string = singapore.get_url(timestamp);
    assert_eq!(query_string, "https://eservices.mas.gov.sg/api/action/datastore/search.json?resource_id=95932927-c8bc-4e7a-b484-68a66a24edfe&limit=100&filters[end_of_day]=2022-08-26");
    let myanmar = CentralBankOfMyanmar;
    let query_string = myanmar.get_url(timestamp);
    assert_eq!(
        query_string,
        "https://forex.cbm.gov.mm/api/history/26-08-2022"
    );
    let bosnia = CentralBankOfBosniaHerzegovina;
    let query_string = bosnia.get_url(timestamp);
    assert_eq!(
        query_string,
        "https://www.cbbh.ba/CurrencyExchange/GetJson?date=08-26-2022%2000%3A00%3A00"
    );
    let israel = BankOfIsrael;
    let query_string = israel.get_url(timestamp);
    assert_eq!(
        query_string,
        "https://www.boi.org.il/currency.xml?rdate=20220826"
    );
    let ecb = EuropeanCentralBank;
    let query_string = ecb.get_url(timestamp);
    assert_eq!(
        query_string,
        "https://www.ecb.europa.eu/stats/eurofxref/eurofxref-daily.xml"
    );
    let canada = BankOfCanada;
    let query_string = canada.get_url(timestamp);
    assert_eq!(
            query_string,
            "https://www.bankofcanada.ca/valet/observations/group/FX_RATES_DAILY/json?start_date=2022-08-26&end_date=2022-08-26"
        );
    let uzbekistan = CentralBankOfUzbekistan;
    let query_string = uzbekistan.get_url(timestamp);
    assert_eq!(
        query_string,
        "https://cbu.uz/ru/arkhiv-kursov-valyut/json/all/2022-08-26/"
    );
}

/// The function tests if the [MonetaryAuthorityOfSingapore] struct returns the correct forex rate.
#[test]
fn extract_rate_from_singapore() {
    let singapore = MonetaryAuthorityOfSingapore;
    let query_response = load_file("test-data/forex/monetary-authority-of-singapore.json");
    let timestamp: u64 = 1656374400;
    let extracted_rates = singapore.extract_rate(&query_response, timestamp);

    assert!(matches!(extracted_rates, Ok(rates) if rates["EUR"] == 1_058_173_944));
}

/// The function tests if the [CentralBankOfMyanmar] struct returns the correct forex rate.
#[test]
fn extract_rate_from_myanmar() {
    let myanmar = CentralBankOfMyanmar;
    let query_response = load_file("test-data/forex/central-bank-of-myanmar.json");
    let timestamp: u64 = 1656374400;
    let extracted_rates = myanmar.extract_rate(&query_response, timestamp);
    assert!(matches!(extracted_rates, Ok(rates) if rates["EUR"] == 1_059_297_297));
}

/// The function tests if the [CentralBankOfBosniaHerzegovina] struct returns the correct forex rate.
#[test]
fn extract_rate_from_bosnia() {
    let bosnia = CentralBankOfBosniaHerzegovina;
    let query_response = load_file("test-data/forex/central-bank-of-bosnia-herzegovina.json");
    let timestamp: u64 = 1656374400;
    let extracted_rates = bosnia.extract_rate(&query_response, timestamp);
    assert!(matches!(extracted_rates, Ok(rates) if rates["EUR"] == 1_057_200_262));
}

/// The function tests if the [BankOfIsrael] struct returns the correct forex rate.
#[test]
fn extract_rate_from_israel() {
    let israel = BankOfIsrael;
    let query_response = load_file("test-data/forex/bank-of-israel.xml");
    let timestamp: u64 = 1656374400;
    let extracted_rates = israel.extract_rate(&query_response, timestamp);

    assert!(matches!(extracted_rates, Ok(rates) if rates["EUR"] == 1_057_916_181));
}

/// The function tests if the [EuropeanCentralBank] struct returns the correct forex rate.
#[test]
fn extract_rate_from_ecb() {
    let ecb = EuropeanCentralBank;
    let query_response = load_file("test-data/forex/central-bank-of-europe.xml");
    let timestamp: u64 = 1664755200;
    let extracted_rates = ecb.extract_rate(&query_response, timestamp);

    assert!(matches!(extracted_rates, Ok(rates) if rates["EUR"] == 976_400_000));
}

/// The function tests if the [BankOfCanada] struct returns the correct forex rate.
#[test]
fn extract_rate_from_canada() {
    let canada = BankOfCanada;
    let query_response = load_file("test-data/forex/bank-of-canada.json");
    let timestamp: u64 = 1656374400;
    let extracted_rates = canada.extract_rate(&query_response, timestamp);

    assert!(matches!(extracted_rates, Ok(rates) if rates["EUR"] == 1_052_938_432));
}

/// The function tests if the [CentralBankOfUzbekistan] struct returns the correct forex rate.
#[test]
fn extract_rate_from_uzbekistan() {
    let uzbekistan = CentralBankOfUzbekistan;
    let query_response = load_file("test-data/forex/central-bank-of-uzbekistan.json");
    let timestamp: u64 = 1656374400;
    let extracted_rates = uzbekistan.extract_rate(&query_response, timestamp);

    assert!(matches!(extracted_rates, Ok(rates) if rates["EUR"] == 1_056_900_158));
}

/// Tests that the [OneDayRatesCollector] struct correctly collects rates and returns them.
#[test]
fn one_day_rate_collector_update_and_get() {
    // Create a collector, update three times, check median rates.
    let mut collector = OneDayRatesCollector {
        rates: HashMap::new(),
        timestamp: 1234,
        sources: HashSet::new(),
    };

    // Insert real values with the correct timestamp.
    let rates = hashmap! {
        "EUR".to_string() => 1_000_000_000,
        "SGD".to_string() => 100_000_000,
        "CHF".to_string() => 700_000_000,
    };
    collector.update("src1".to_string(), rates);
    let rates = hashmap! {
        "EUR".to_string() => 1_100_000_000,
        "SGD".to_string() => 1_000_000_000,
        "CHF".to_string() => 1_000_000_000,
    };
    collector.update("src2".to_string(), rates);
    let rates = hashmap! {
        "EUR".to_string() => 800_000_000,
        "SGD".to_string() => 1_300_000_000,
        "CHF".to_string() => 2_100_000_000,
    };
    collector.update("src3".to_string(), rates);

    let result = collector.get_rates_map();
    assert_eq!(result.len(), 3);
    result.values().for_each(|v| {
        let rate: ExchangeRate = v.clone().into();
        assert_eq!(rate.rate, RATE_UNIT);
        assert_eq!(rate.metadata.base_asset_num_received_rates, 3);
    });
}

/// Tests that the [ForexRatesCollector] struct correctly collects rates and returns them.
#[test]
fn rate_collector_update_and_get() {
    let mut collector = ForexRatesCollector::new();

    // Start by executing the same logic as for the [OneDayRatesCollector] to verify that the calls are relayed correctly
    let first_day_timestamp = (123456789 / SECONDS_PER_DAY) * SECONDS_PER_DAY;
    let rates = hashmap! {
        "EUR".to_string() => 1_000_000_000,
        "SGD".to_string() => 100_000_000,
        "CHF".to_string() => 700_000_000,
    };
    collector.update("src1".to_string(), first_day_timestamp, rates);
    let rates = hashmap! {
        "EUR".to_string() => 1_100_000_000,
        "SGD".to_string() => 1_000_000_000,
        "CHF".to_string() => 1_000_000_000,
    };
    collector.update("src2".to_string(), first_day_timestamp, rates);
    let rates = hashmap! {
        "EUR".to_string() => 800_000_000,
        "SGD".to_string() => 1_300_000_000,
        "CHF".to_string() => 2_100_000_000,
    };
    collector.update("src3".to_string(), first_day_timestamp, rates);

    let result = collector.get_rates_map(first_day_timestamp).unwrap();
    assert_eq!(result.len(), 3);
    result.values().for_each(|v| {
        let rate: ExchangeRate = v.clone().into();
        assert_eq!(rate.rate, RATE_UNIT);
        assert_eq!(rate.metadata.base_asset_num_received_rates, 3);
    });

    // Add a new day
    let second_day_timestamp = first_day_timestamp + SECONDS_PER_DAY;
    let test_rate: u64 = 700_000_000;
    let rates = hashmap! {
        "EUR".to_string() => test_rate,
        "SGD".to_string() => test_rate,
        "CHF".to_string() => test_rate,
    };
    collector.update("src1".to_string(), second_day_timestamp, rates);
    let result = collector.get_rates_map(second_day_timestamp).unwrap();
    assert_eq!(result.len(), 3);
    result.values().for_each(|v| {
        let rate: ExchangeRate = v.clone().into();
        assert_eq!(rate.rate, test_rate);
        assert_eq!(rate.metadata.base_asset_num_received_rates, 1);
    });

    // Add a third day and expect the first one to not be available
    let third_day_timestamp = second_day_timestamp + SECONDS_PER_DAY;
    let test_rate: u64 = 800_000_000;
    let rates = hashmap! {
        "EUR".to_string() => test_rate,
        "SGD".to_string() => test_rate,
        "CHF".to_string() => test_rate,
    };
    collector.update("src1".to_string(), third_day_timestamp, rates.clone());
    let result = collector.get_rates_map(third_day_timestamp).unwrap();
    assert_eq!(result.len(), 3);
    result.values().for_each(|v| {
        let rate: ExchangeRate = v.clone().into();
        assert_eq!(rate.rate, test_rate);
        assert_eq!(rate.metadata.base_asset_num_received_rates, 1);
    });
    assert!(collector.get_rates_map(first_day_timestamp).is_none());
    assert!(collector.get_rates_map(second_day_timestamp).is_some());

    // Try to add an old day and expect it to fail
    assert!(!collector.update("src1".to_string(), first_day_timestamp, rates));
}

/// Tests that the [ForexRatesStore] struct correctly updates rates for the same timestamp.
#[test]
fn rate_store_update() {
    // Create a store, update, check that only rates with more sources were updated.
    let mut store = ForexRateStore::new();
    store.put(
        1234,
        hashmap! {
            "EUR".to_string() =>
                QueriedExchangeRate {
                    base_asset: Asset {
                        symbol: "EUR".to_string(),
                        class: AssetClass::FiatCurrency,
                    },
                    quote_asset: Asset {
                        symbol: USD.to_string(),
                        class: AssetClass::FiatCurrency,
                    },
                    timestamp: 1234,
                    rates: vec![800_000_000],
                    base_asset_num_queried_sources: 4,
                    base_asset_num_received_rates: 4,
                    quote_asset_num_queried_sources: 4,
                    quote_asset_num_received_rates: 4,
                },
            "SGD".to_string() =>
                QueriedExchangeRate {
                    base_asset: Asset {
                        symbol: "SGD".to_string(),
                        class: AssetClass::FiatCurrency,
                    },
                    quote_asset: Asset {
                        symbol: USD.to_string(),
                        class: AssetClass::FiatCurrency,
                    },
                    timestamp: 1234,
                    rates: vec![1_000_000_000],
                    base_asset_num_queried_sources: 5,
                    base_asset_num_received_rates: 5,
                    quote_asset_num_queried_sources: 5,
                    quote_asset_num_received_rates: 5,
                },
            "CHF".to_string() =>
                QueriedExchangeRate {
                    base_asset: Asset {
                        symbol: "CHF".to_string(),
                        class: AssetClass::FiatCurrency,
                    },
                    quote_asset: Asset {
                        symbol: USD.to_string(),
                        class: AssetClass::FiatCurrency,
                    },
                    timestamp: 1234,
                    rates: vec![2_100_000_000],
                    base_asset_num_queried_sources: 2,
                    base_asset_num_received_rates: 2,
                    quote_asset_num_queried_sources: 2,
                    quote_asset_num_received_rates: 2,
                },
        },
    );
    store.put(
        1234,
        hashmap! {
            "EUR".to_string() =>
                QueriedExchangeRate {
                    base_asset: Asset {
                        symbol: "EUR".to_string(),
                        class: AssetClass::FiatCurrency,
                    },
                    quote_asset: Asset {
                        symbol: USD.to_string(),
                        class: AssetClass::FiatCurrency,
                    },
                    timestamp: 1234,
                    rates: vec![1_000_000_000],
                    base_asset_num_queried_sources: 5,
                    base_asset_num_received_rates: 5,
                    quote_asset_num_queried_sources: 5,
                    quote_asset_num_received_rates: 5,
                },
            "GBP".to_string() =>
                QueriedExchangeRate {
                    base_asset: Asset {
                        symbol: "GBP".to_string(),
                        class: AssetClass::FiatCurrency,
                    },
                    quote_asset: Asset {
                        symbol: USD.to_string(),
                        class: AssetClass::FiatCurrency,
                    },
                    timestamp: 1234,
                    rates: vec![1_000_000_000],
                    base_asset_num_queried_sources: 2,
                    base_asset_num_received_rates: 2,
                    quote_asset_num_queried_sources: 2,
                    quote_asset_num_received_rates: 2,
                },
            "CHF".to_string() =>
                QueriedExchangeRate {
                    base_asset: Asset {
                        symbol: "CHF".to_string(),
                        class: AssetClass::FiatCurrency,
                    },
                    quote_asset: Asset {
                        symbol: USD.to_string(),
                        class: AssetClass::FiatCurrency,
                    },
                    timestamp: 1234,
                    rates: vec![1_000_000_000],
                    base_asset_num_queried_sources: 5,
                    base_asset_num_received_rates: 5,
                    quote_asset_num_queried_sources: 5,
                    quote_asset_num_received_rates: 5,
                },
        },
    );

    assert!(matches!(
        store.get(1234, 1234, "EUR", USD),
        Ok(rate) if rate.rates == vec![1_000_000_000] && rate.base_asset_num_received_rates == 5,
    ));
    assert!(matches!(
        store.get(1234, 1234, "SGD", USD),
        Ok(rate) if rate.rates == vec![1_000_000_000] && rate.base_asset_num_received_rates == 5,
    ));
    assert!(matches!(
        store.get(1234, 1234, "CHF", USD),
        Ok(rate) if rate.rates == vec![1_000_000_000] && rate.base_asset_num_received_rates == 5,
    ));
    assert!(matches!(
        store.get(1234, 1234, "GBP", USD),
        Ok(rate) if rate.rates == vec![1_000_000_000] && rate.base_asset_num_received_rates == 2,
    ));

    assert!(matches!(
        store.get(1234, 1234, "CHF", "EUR"),
        Ok(rate) if rate.rates == vec![1_000_000_000] && rate.base_asset_num_received_rates == 5 && rate.base_asset.symbol == "CHF" && rate.quote_asset.symbol == "EUR",
    ));

    let result = store.get(1234, 1234, "HKD", USD);
    assert!(
        matches!(result, Err(GetForexRateError::CouldNotFindBaseAsset(timestamp, ref asset)) if timestamp == (1234 / SECONDS_PER_DAY) * SECONDS_PER_DAY && asset == "HKD"),
        "Expected `Err(GetForexRateError::CouldNotFindBaseAsset)`, Got: {:?}",
        result
    );
}

#[test]
fn rate_store_get_same_asset() {
    let store = ForexRateStore::new();
    let result: Result<ExchangeRate, GetForexRateError> =
        store.get(1234, 1234, USD, USD).map(|v| v.into());
    assert!(matches!(result, Ok(forex_rate) if forex_rate.rate == RATE_UNIT));
    let result: Result<ExchangeRate, GetForexRateError> =
        store.get(1234, 1234, "CHF", "CHF").map(|v| v.into());
    assert!(matches!(result, Ok(forex_rate) if forex_rate.rate == RATE_UNIT));
}

/// Test that SDR and XDR rates are reported as the same asset under the symbol "xdr"
#[test]
fn collector_sdr_xdr() {
    let mut collector = OneDayRatesCollector {
        rates: HashMap::new(),
        timestamp: 1234,
        sources: HashSet::new(),
    };

    let rates = vec![
        ("SDR".to_string(), 1_000_000_000),
        ("XDR".to_string(), 700_000_000),
    ]
    .into_iter()
    .collect();
    collector.update("src1".to_string(), rates);

    let rates = vec![("SDR".to_string(), 1_100_000_000)]
        .into_iter()
        .collect();
    collector.update("src2".to_string(), rates);

    let rates = vec![
        ("SDR".to_string(), 1_050_000_000),
        ("XDR".to_string(), 900_000_000),
    ]
    .into_iter()
    .collect();
    collector.update("src3".to_string(), rates);

    let result: ExchangeRate = (&collector.get_rates_map()["XDR"]).clone().into();

    assert!(matches!(
        result,
        rate if rate.rate == RATE_UNIT && rate.metadata.base_asset_num_received_rates == 5,
    ))
}

/// Tests that the [ForexRatesCollector] computes and adds the correct CXDR rate if
/// all EUR/USD, CNY/USD, JPY/USD, and GBP/USD rates are available.
#[test]
fn verify_compute_xdr_rate() {
    let mut map: HashMap<String, Vec<u64>> = HashMap::new();
    map.insert(
        "EUR".to_string(),
        vec![979_500_000, 981_500_000, 969_800_000],
    ); // median: 979_500_000
    map.insert("CNY".to_string(), vec![140_500_000, 148_900_000]); // median: 144_700_000
    map.insert(
        "JPY".to_string(),
        vec![6_900_000, 7_100_000, 6_800_000, 7_000_000],
    ); // median: 6_950_000
    map.insert(
        "GBP".to_string(),
        vec![1_121_200_000, 1_122_000_000, 1_120_900_000],
    ); // median: 1_121_200_000

    let collector = OneDayRatesCollector {
        rates: map,
        timestamp: 0,
        sources: HashSet::new(),
    };

    let rates_map = collector.get_rates_map();
    let cxdr_usd_rate: ExchangeRate = rates_map
        .get(COMPUTED_XDR_SYMBOL)
        .expect("A rate should be returned")
        .clone()
        .into();

    // The expected CXDR/USD rate is
    // 0.58252*1.0+0.38671*0.9795+1.0174*0.1447+11.9*0.00695+0.085946*1.1212
    // = 1.28758788

    // The expected variance is
    // EUR_XDR_WEIGHT^2*Var(EUR) + CNY_XDR_WEIGHT^2*Var(CNY)
    // + JPY_XDR_WEIGHT^2*Var(JPY) + GBP_XDR_WEIGHT^2*Var(GBP) or, equivalently
    // (EUR_XDR_WEIGHT*std_dev(EUR))^2 + (CNY_XDR_WEIGHT*std_dev(CNY))^2
    // + (JPY_XDR_WEIGHT*std_dev(JPY))^2 + (GBP_XDR_WEIGHT*std_dev(GBP))^2, which is
    // (0.386710*0.006258061)^2 + (1.0174*0.005939696)^2 + (11.9*0.000129099)^2 + (0.085946*0.000568624)^2
    // = 0.006688618.
    // The standard deviation is sqrt(0.000044738) = 0.00065.

    let _expected_rate = ExchangeRate {
        base_asset: Asset {
            symbol: "CXDR".to_string(),
            class: AssetClass::FiatCurrency,
        },
        quote_asset: Asset {
            symbol: USD.to_string(),
            class: AssetClass::FiatCurrency,
        },
        timestamp: 0,
        rate: 1287587880,
        metadata: ExchangeRateMetadata {
            decimals: DECIMALS,
            base_asset_num_queried_sources: FOREX_SOURCES.len(),
            base_asset_num_received_rates: 2,
            quote_asset_num_queried_sources: FOREX_SOURCES.len(),
            quote_asset_num_received_rates: 2,
            standard_deviation: 6688618,
        },
    };

    assert_eq!(cxdr_usd_rate, _expected_rate);
}

/// Test transform_http_response_body to the correct set of bytes.
#[test]
fn encoding_transformed_http_response() {
    let forex = Forex::BankOfIsrael(BankOfIsrael);
    let body = "\u{feff}<?xml version=\"1.0\" encoding=\"utf-8\" standalone=\"yes\"?><CURRENCIES>  <LAST_UPDATE>2022-06-28</LAST_UPDATE>  <CURRENCY>    <NAME>Dollar</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>USD</CURRENCYCODE>    <COUNTRY>USA</COUNTRY>    <RATE>3.436</RATE>    <CHANGE>1.148</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Pound</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>GBP</CURRENCYCODE>    <COUNTRY>Great Britain</COUNTRY>    <RATE>4.2072</RATE>    <CHANGE>0.824</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Yen</NAME>    <UNIT>100</UNIT>    <CURRENCYCODE>JPY</CURRENCYCODE>    <COUNTRY>Japan</COUNTRY>    <RATE>2.5239</RATE>    <CHANGE>0.45</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Euro</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>EUR</CURRENCYCODE>    <COUNTRY>EMU</COUNTRY>    <RATE>3.6350</RATE>    <CHANGE>1.096</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Dollar</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>AUD</CURRENCYCODE>    <COUNTRY>Australia</COUNTRY>    <RATE>2.3866</RATE>    <CHANGE>1.307</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Dollar</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>CAD</CURRENCYCODE>    <COUNTRY>Canada</COUNTRY>    <RATE>2.6774</RATE>    <CHANGE>1.621</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>krone</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>DKK</CURRENCYCODE>    <COUNTRY>Denmark</COUNTRY>    <RATE>0.4885</RATE>    <CHANGE>1.097</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Krone</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>NOK</CURRENCYCODE>    <COUNTRY>Norway</COUNTRY>    <RATE>0.3508</RATE>    <CHANGE>1.622</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Rand</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>ZAR</CURRENCYCODE>    <COUNTRY>South Africa</COUNTRY>    <RATE>0.2155</RATE>    <CHANGE>0.701</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Krona</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>SEK</CURRENCYCODE>    <COUNTRY>Sweden</COUNTRY>    <RATE>0.3413</RATE>    <CHANGE>1.276</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Franc</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>CHF</CURRENCYCODE>    <COUNTRY>Switzerland</COUNTRY>    <RATE>3.5964</RATE>    <CHANGE>1.416</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Dinar</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>JOD</CURRENCYCODE>    <COUNTRY>Jordan</COUNTRY>    <RATE>4.8468</RATE>    <CHANGE>1.163</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Pound</NAME>    <UNIT>10</UNIT>    <CURRENCYCODE>LBP</CURRENCYCODE>    <COUNTRY>Lebanon</COUNTRY>    <RATE>0.0227</RATE>    <CHANGE>0.889</CHANGE>  </CURRENCY>  <CURRENCY>    <NAME>Pound</NAME>    <UNIT>1</UNIT>    <CURRENCYCODE>EGP</CURRENCYCODE>    <COUNTRY>Egypt</COUNTRY>    <RATE>0.1830</RATE>    <CHANGE>1.049</CHANGE>  </CURRENCY></CURRENCIES>".as_bytes();
    let context_bytes = forex
        .encode_context(&ForexContextArgs {
            timestamp: 1656374400,
        })
        .expect("should be able to encode");
    let context = Forex::decode_context(&context_bytes).expect("should be able to decode bytes");
    let bytes = forex
        .transform_http_response_body(body, &context.payload)
        .expect("should be able to transform the body");
    let result = Forex::decode_response(&bytes);

    assert!(matches!(result, Ok(map) if map["EUR"] == 1_057_916_181));
}

/// Test that response decoding works correctly.
#[test]
fn decode_transformed_http_response() {
    let hex_string = "4449444c026d016c0200710178010001034555520100000000000000";
    let bytes = hex::decode(hex_string).expect("should be able to decode");
    let result = Forex::decode_response(&bytes);
    assert!(matches!(result, Ok(map) if map["EUR"] == 1));
}

/// This function tests that the [ForexRateStore] can return the amount of bytes it has
/// allocated over time.
#[test]
fn forex_rate_store_can_return_the_number_of_bytes_allocated_to_it() {
    let mut store = ForexRateStore::new();

    store.put(
        0,
        hashmap! {
            "EUR".to_string() => QueriedExchangeRate {
                base_asset: Asset {
                    symbol: "EUR".to_string(),
                    class: AssetClass::FiatCurrency,
                },
                quote_asset: Asset {
                    symbol: USD.to_string(),
                    class: AssetClass::FiatCurrency,
                },
                timestamp: 1234,
                rates: vec![10_000],
                base_asset_num_queried_sources: 5,
                base_asset_num_received_rates: 5,
                quote_asset_num_queried_sources: 5,
                quote_asset_num_received_rates: 5,
            }
        },
    );

    assert_eq!(store.allocated_bytes(), 273);
}

/// This functiont ests the the forexes can report the max response bytes needed
/// to make a successful HTTP outcall.
#[test]
fn forex_max_response_bytes() {
    let forex = Forex::MonetaryAuthorityOfSingapore(MonetaryAuthorityOfSingapore);
    assert_eq!(forex.max_response_bytes(), 3 * ONE_KIB);
    let forex = Forex::CentralBankOfMyanmar(CentralBankOfMyanmar);
    assert_eq!(forex.max_response_bytes(), 3 * ONE_KIB);
    let forex = Forex::CentralBankOfBosniaHerzegovina(CentralBankOfBosniaHerzegovina);
    assert_eq!(forex.max_response_bytes(), 30 * ONE_KIB);
    let forex = Forex::BankOfIsrael(BankOfIsrael);
    assert_eq!(forex.max_response_bytes(), 10 * ONE_KIB);
    let forex = Forex::EuropeanCentralBank(EuropeanCentralBank);
    assert_eq!(forex.max_response_bytes(), 3 * ONE_KIB);
    let forex = Forex::BankOfCanada(BankOfCanada);
    assert_eq!(forex.max_response_bytes(), 10 * ONE_KIB);
    let forex = Forex::CentralBankOfUzbekistan(CentralBankOfUzbekistan);
    assert_eq!(forex.max_response_bytes(), 30 * ONE_KIB);
}

#[test]
#[cfg(not(feature = "ipv4-support"))]
fn is_available() {
    let available_forex_sources_count = FOREX_SOURCES.iter().filter(|e| e.is_available()).count();
    assert_eq!(available_forex_sources_count, 4);
}

#[test]
#[cfg(feature = "ipv4-support")]
fn is_available_ipv4() {
    let available_forex_sources_count = FOREX_SOURCES.iter().filter(|e| e.is_available()).count();
    assert_eq!(available_forex_sources_count, 7);
}
