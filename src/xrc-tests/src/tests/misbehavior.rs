use std::collections::HashMap;

use ic_xrc_types::{
    Asset, AssetClass, ExchangeRate, ExchangeRateMetadata, GetExchangeRateRequest,
    GetExchangeRateResult,
};
use maplit::hashmap;

use crate::tests::{NUM_EXCHANGES, NUM_FOREX_SOURCES};
use crate::{
    container::{run_scenario, Container},
    mock_responses, ONE_DAY_SECONDS,
};

/// This value is derived in the basic_exchange_rates crypto pair portion of the test.
const CRYPTO_PAIR_BASIC_STD_DEV: u64 = 3_483_761;

/// This value is derived in the basic_exchange_rates crypto fiat pair portion of the test.
const CRYPTO_FIAT_PAIR_BASIC_STD_DEV: u64 = 80_650_883;

/// This value is derived in the basic_exchange_rates fiat crypto pair portion of the test.
const FIAT_CRYPTO_PAIR_BASIC_STD_DEV: u64 = 1_474_512;

/// This value is derived from using the common mock dataset (mock_responses::forex::build_common_responses).
/// A full explanation how on the number is derived can be seen starting in the basic_exchange_rate test on line
/// 119.
const FIAT_PAIR_COMMON_DATASET_STD_DEV: u64 = 396_623_626;

/// Setup:
/// * Deploy mock FOREX data providers and exchanges, some of which are configured to be malicious
/// * Start replicas and deploy the XRC, configured to use the mock data sources
///
/// Runbook:
/// * Request exchange rate for various cryptocurrency and fiat currency pairs
/// * Assert that the returned rates correspond to the expected values and that the confidence is lower due to the erroneous responses
///
/// Success criteria:
/// * All queries return the expected values
///
/// The expected values are determined as follows:
///
/// Crypto-pair (retrieve ICP/BTC rate)
/// 0. The XRC retrieves the ICP/USDT rate.
///     1. ICP/USDT rates: [3900000000, 3900000000, 3911000000, 3930000000, 4005000000]
///         1. There are only 5 rates as the other 3 have been filtered out as they were >= 20% different
///            than the median rate.
/// 1. The XRC retrieves the BTC/USDT rate.
///     1. BTC/USDT rates: [42030000000, 42640000000, 45000000000, 46022000000, 46101000000]
///         1. There are only 5 rates as the other 3 have been filtered out as they were >= 20% different
///            than the median rate.
/// 2. The XRC divides ICP/USDT by BTC/USDT. The division inverts BTC/USDT to USDT/BTC then multiplies ICP/USDT and USDT/BTC
///    to get the resulting ICP/BTC rate.
///     1. ICP/BTC rates:
///        [84596861, 84596861, 84742078, 84742078, 84835468, 84981094, 85247606, 85393940, 86666665, 86666665,
///        86874469, 86911110, 87023595, 87333332, 88999999, 91463412, 91463412, 91721386, 92166977, 92790863,
///        92790863, 93052580, 93504638, 93925888, 95289078]
/// 3. The XRC returns the median rate and the standard deviation from the BTC/ICP rates.
///     1. The median rate from step 2 is 87023595.
///     2. The standard deviation from step 2 is 3644799.
///
/// Crypto-fiat pair (retrieve BTC/EUR rate)
/// 0. The XRC retrieves rates from the mock forex sources.
///     1. During collection the rates retrieved are normalized to USD.
///     2. When the collected rates are normalized, then the EUR rate (EUR/USD) is collected (for more information on this collection, see xrc/forex.rs:483).
///         1. For all requests in the following test, this should result in a EUR/USD with the following rates:
///            [917777444, 976400000, 1056100000, 1056900158, 1057200262, 1058516154]
///         2. Large values are filtered out as they were greater than the median rate.
/// 1. The XRC retrieves the BTC/USDT rates from the mock exchange responses (request 1 responses).
///     1. BTC/USDT rates: [42030000000, 42640000000, 46022000000, 46101000000]
///         1. There are only 4 rates as the other 3 have been filtered out as they were greater
///            than the median rate.
/// 2. The XRC retrieves the stablecoin rates from the mock exchanges.
///     1.  DAI:  [ 950000000, 990000000, 990000000, 1000000000, 1020000000, 1030927835 ]
///     2. USDC: [ 950000000, 990000000, 990000000, 1000000000, 1020000000, 1030927835 ]
/// 3. The XRC determines the USDT/USD rate.
///     1. USDT/USD: [ 970000000, 980392156, 1000000000, 1010101010, 1010101010, 1052631578 ]
/// 4. The XRC then multiplies the USDT/USD rate (step 3) with the ICP/USDT rate (step 1) to get the BTC/USD rate.
///     1. This results in the following rates:
///        [40701200000, 40769100000, 41137254865, 41205882316, 41360800000, 41803921531, 41960000000,
///        42030000000, 42383838379, 42383838379, 42454545450, 42454545450, 42640000000, 42922500000,
///        43070707066, 43070707066, 43382352903, 43488010000, 43953921529, 44168421012, 44242105223,
///        44250000000, 44641340000, 44696969692, 44696969692, 44717970000, 44833000000, 44884210485,
///        45119607803, 45197058783, 45285858581, 45285858581, 46022000000, 46101000000, 46486868682,
///        46486868682, 46566666662, 46566666662, 46578947326, 47192631536, 48444210482, 48527368377]
/// 5. The XRC divides the BTC/USD by the forex rate EUR/USD. The division works by inverting EUR/USD to USD/EUR then multiplying
///    USD/EUR and BTC/USD resulting in BTC/EUR.
///     1. This results in the following rates:
///        [37668988975, 38072558056, 38215695691, 38515330948, 38563270803, 38574220730, 38603446631, 38625121949, 38834009252, 38927967367,
///        38976420829, 38987488069, 39017027083, 39074321000, 39122956627, 39134065476, 39163715544, 39226271968, 39226271968, 39397624424,
///        39492946194, 39542102882, 39553330746, 39583298473, 39706526750, 39755949281, 39767237866, 39795580221, 39795580221, 39797367660,
///        40107602774, 40107602774, 40157524522, 40157524522, 40168927134, 40168927134, 40199361269, 40199361269, 40282805154, 40330823314,
///        40332944976, 40344397398, 40374964479, 40689702172, 40689702172, 40740348456, 40740348456, 40751916559, 40751916559, 40762910125,
///        40782792398, 40782792398, 40877904439, 41236971036, 41246781123, 41288298504, 41300022195, 41317584124, 41331313309, 41471183566,
///        41578168365, 41678765919, 41688681106, 41730643286, 41742492580, 41754506310, 41760242659, 41774118933, 41796343910, 41848367626,
///        41860250348, 41891965920, 41998149859, 41998149859, 42173508467, 42201845839, 42226001639, 42237991588, 42245902261, 42269993358,
///        42298485541, 42310496072, 42342552775, 42360507948, 42402952755, 42455731515, 42467786695, 42499962570, 42512341275, 42522454766,
///        42565256190, 42577342470, 42595447550, 42609601350, 42625337002, 42678392562, 42690510967, 42698506391, 42722855590, 42751653025,
///        42763792232, 42796192376, 42814339913, 42941758859, 42941758859, 42951974507, 42951974507, 42995208268, 42995208268, 43007416632,
///        43007416632, 43025704592, 43025704592, 43040001359, 43040001359, 43045882794, 43477843781, 43480689686, 43480689686, 43531960452,
///        43544321225, 43552476558, 43577312740, 43606686124, 43619068115, 43652116263, 43670626751, 43766492976, 43917013915, 43917013915,
///        43971677220, 43971677220, 43984162849, 43984162849, 43992400559, 43992400559, 44017487612, 44017487612, 44047157697, 44047157697,
///        44059664758, 44059664758, 44093046725, 44093046725, 44111744188, 44111744188, 44421553645, 44705038876, 44749832880, 44760478660,
///        44805532791, 44818255191, 44837313170, 44852211907, 44897466753, 45066263322, 45183989122, 45311455532, 45549083567, 45720339981,
///        45766151306, 45795416129, 45798822161, 45823116224, 45836127564, 45844712124, 45870855474, 45901774826, 45914808501, 45949596024,
///        45969080748, 46087668945, 46210167718, 46257996085, 46257996085, 46289490721, 46460065280, 46553200949, 46553200949, 46929358864,
///        46929358864, 47134371115, 47215280578, 47560549941, 47610475869, 47610475869, 47692202599, 47692202599, 48070092884, 48205701144,
///        48513335687, 48640702875, 48724198063, 48905331829, 49031494785, 49161818104, 49246207822, 49526762404, 49526762404, 49615127444,
///        49700295300, 50145054511, 50231132024, 50651570207, 50651570207, 50738517190, 50738517190, 51612099727, 52784267858, 52874875766]
/// 6. The XRC then returns the median and the standard deviation.
///     1. The median rate from step 5 is 42946866683.
///     2. The standard deviation from step 5 is 3202501200.
///
/// Fiat-crypto pair (retrieve EUR/BTC rate)
/// 0. The instructions are similar to the crypto-fiat pair. The only difference is that the rates are inverted before
///    being returned.
///     1. When inverted, the median rate is 23284585.
///     2. When inverted, the standard deviation is 1650102.
///
/// Fiat pair (retrieve EUR/JPY rate)
/// 0. The XRC retrieves rates from the mock forex sources.
///     1. During collection the rates retrieved are normalized to USD.
///     2. When the collected rates are normalized, then the EUR rate (EUR/USD) is collected (for more information on this collection, see xrc/forex.rs:483).
///         1. For all requests in the following test, this should result in a EUR/USD with the following rates:
///             [917777444, 976400000, 1056100000, 1056900158, 1057200262, 1058516154]
///         2. Large values are filtered out as they were greater than the median rate.
///         3. For all requests in the following test, this should result in a EUR/USD with the following rates:
///             [6900840, 7217610, 7350873, 7380104, 7395293, 7395930]
///         4. Large values are filtered out as they were greater than the median rate.
/// 1. The XRC divides EUR/USD by JPY/USD. The division inverts JPY/USD to USD/JPY then multiplies EUR/USD and USD/JPY
///    to get the resulting EUR/JPY rate.
///     1. EUR/JPY rates should then include:
///        [124092229644, 124102918437, 124358334787, 124852849994, 132018556151, 132029927684, 132301658621, 132827760729, 132995033068, 135589467313,
///        141490021504, 142794753330, 142807053080, 142902942293, 142915251362, 142943519205, 142955831769, 143100964430, 143121440305, 143133768195,
///        143209385395, 143250049321, 143428351957, 143670010350, 143778862455, 143819688082, 143998699745, 144250173885, 146337074309, 146349679180,
///        146650881613, 147234043901, 153039340138, 153155290949, 153198778988, 153389464760, 156024793773, 156143006525, 156187342918, 156381748540,
///        156835799409, 159895313434 ]
/// 2. The XRC then return the median and the standard deviation.
///     1. The median rate from the group of rates in step 1.a.: 143229717358.
///     2. The standard deviation of the group of rates in step 1.a.: 9438739634.
#[ignore]
#[test]
fn misbehavior() {
    let now_seconds = time::OffsetDateTime::now_utc().unix_timestamp() as u64;
    let yesterday_timestamp_seconds = now_seconds
        .saturating_sub(ONE_DAY_SECONDS)
        .saturating_div(ONE_DAY_SECONDS)
        .saturating_mul(ONE_DAY_SECONDS);
    let timestamp_seconds = now_seconds / 60 * 60;

    let responses = mock_responses::exchanges::build_responses(
        "ICP".to_string(),
        timestamp_seconds,
        |exchange| match exchange {
            xrc::Exchange::Coinbase(_) => Some("100000.92"),
            xrc::Exchange::KuCoin(_) => Some("0.0000000001"),
            xrc::Exchange::Okx(_) => Some("3.90"),
            xrc::Exchange::GateIo(_) => Some("3.90"),
            xrc::Exchange::Mexc(_) => Some("3.911"),
            xrc::Exchange::Poloniex(_) => Some("4.005"),
            xrc::Exchange::CryptoCom(_) => Some("100000.0"),
            xrc::Exchange::Bitget(_) => Some("3.93"),
            xrc::Exchange::Digifinex(_) => Some("1000.00"),
        },
    )
    .chain(mock_responses::exchanges::build_responses(
        "BTC".to_string(),
        timestamp_seconds,
        |exchange| match exchange {
            xrc::Exchange::Coinbase(_) => Some("10000.25"),
            xrc::Exchange::KuCoin(_) => Some("10000.833"),
            xrc::Exchange::Okx(_) => Some("42.03"),
            xrc::Exchange::GateIo(_) => Some("42.64"),
            xrc::Exchange::Mexc(_) => Some("46.101"),
            xrc::Exchange::Poloniex(_) => Some("46.022"),
            xrc::Exchange::CryptoCom(_) => Some("10000.96000000"),
            xrc::Exchange::Bitget(_) => Some("45.00"),
            xrc::Exchange::Digifinex(_) => Some("1000.50")
        },
    ))
    .chain(mock_responses::stablecoin::build_responses(
        timestamp_seconds,
    ))
    .chain(mock_responses::forex::build_responses(
        now_seconds,
        |forex| match forex {
            xrc::Forex::CentralBankOfMyanmar(_) => {
                Some(hashmap! { "EUR" => "1.0", "JPY" => "10000.0" })
            }
            xrc::Forex::BankOfCanada(_) => Some(hashmap! { "EUR" => "50.0", "JPY" => "0.10" }),
            xrc::Forex::ReserveBankOfAustralia(_) => {
                Some(hashmap! { "EUR" => "10.0", "JPY" => "200.0" })
            }
            xrc::Forex::SwissFederalOfficeForCustoms(_) => {
                Some(hashmap! { "EUR" => "5.00", "JPY" => "1.00" })
            }
            xrc::Forex::CentralBankOfGeorgia(_) => {
                Some(hashmap! { "EUR" => "1000.0", "JPY" => "1000.0" })
            }
            _ => Some(HashMap::new()),
        },
    ))
    .collect::<Vec<_>>();

    let container = Container::builder()
        .name("misbehavior")
        .exchange_responses(responses)
        .build();

    run_scenario(container, |container| {
        let btc_asset = Asset {
            symbol: "BTC".to_string(),
            class: AssetClass::Cryptocurrency,
        };

        let eur_asset = Asset {
            symbol: "EUR".to_string(),
            class: AssetClass::FiatCurrency,
        };

        // Crypto Pair
        let crypto_pair_request = GetExchangeRateRequest {
            timestamp: Some(timestamp_seconds),
            base_asset: Asset {
                symbol: "ICP".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            quote_asset: btc_asset.clone(),
        };
        let expected_crypto_pair_rate = ExchangeRate {
            base_asset: Asset {
                symbol: "ICP".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            quote_asset: btc_asset.clone(),
            timestamp: timestamp_seconds,
            rate: 87_023_595,
            metadata: ExchangeRateMetadata {
                decimals: 9,
                base_asset_num_queried_sources: NUM_EXCHANGES,
                base_asset_num_received_rates: NUM_EXCHANGES,
                quote_asset_num_queried_sources: NUM_EXCHANGES,
                quote_asset_num_received_rates: NUM_EXCHANGES,
                standard_deviation: 3_644_799,
                forex_timestamp: None,
            },
        };

        let crypto_pair_result = container
            .call_canister::<_, GetExchangeRateResult>("get_exchange_rate", &crypto_pair_request)
            .expect("Failed to call canister for rates");
        let exchange_rate =
            crypto_pair_result.expect("Failed to retrieve an exchange rate from the canister.");

        assert_eq!(exchange_rate, expected_crypto_pair_rate);
        assert!(CRYPTO_PAIR_BASIC_STD_DEV < exchange_rate.metadata.standard_deviation);

        // Crypto Fiat Pair
        let crypto_fiat_pair_request = GetExchangeRateRequest {
            timestamp: Some(timestamp_seconds),
            base_asset: btc_asset.clone(),
            quote_asset: eur_asset.clone(),
        };
        let expected_crypto_fiat_pair_rate = ExchangeRate {
            base_asset: btc_asset.clone(),
            quote_asset: eur_asset.clone(),
            timestamp: timestamp_seconds,
            rate: 42_946_866_683,
            metadata: ExchangeRateMetadata {
                decimals: 9,
                base_asset_num_queried_sources: NUM_EXCHANGES,
                base_asset_num_received_rates: NUM_EXCHANGES,
                quote_asset_num_queried_sources: NUM_FOREX_SOURCES,
                quote_asset_num_received_rates: NUM_FOREX_SOURCES,
                standard_deviation: 3_202_501_200,
                forex_timestamp: Some(yesterday_timestamp_seconds),
            },
        };

        let crypto_fiat_pair_result = container
            .call_canister::<_, GetExchangeRateResult>(
                "get_exchange_rate",
                &crypto_fiat_pair_request,
            )
            .expect("Failed to call canister for rates");
        let exchange_rate = crypto_fiat_pair_result
            .expect("Failed to retrieve an exchange rate from the canister.");

        assert_eq!(exchange_rate, expected_crypto_fiat_pair_rate);
        assert!(CRYPTO_FIAT_PAIR_BASIC_STD_DEV < exchange_rate.metadata.standard_deviation);

        // Fiat Crypto Pair
        let fiat_crypto_pair_request = GetExchangeRateRequest {
            timestamp: Some(timestamp_seconds),
            base_asset: eur_asset.clone(),
            quote_asset: btc_asset.clone(),
        };
        let expected_fiat_crypto_pair_rate = ExchangeRate {
            base_asset: eur_asset.clone(),
            quote_asset: btc_asset,
            timestamp: timestamp_seconds,
            rate: 23_284_585,
            metadata: ExchangeRateMetadata {
                decimals: 9,
                base_asset_num_queried_sources: NUM_FOREX_SOURCES,
                base_asset_num_received_rates: NUM_FOREX_SOURCES,
                quote_asset_num_queried_sources: NUM_EXCHANGES,
                quote_asset_num_received_rates: NUM_EXCHANGES,
                standard_deviation: 1_650_102,
                forex_timestamp: Some(yesterday_timestamp_seconds),
            },
        };

        let fiat_crypto_pair_result = container
            .call_canister::<_, GetExchangeRateResult>(
                "get_exchange_rate",
                &fiat_crypto_pair_request,
            )
            .expect("Failed to call canister for rates");
        let exchange_rate = fiat_crypto_pair_result
            .expect("Failed to retrieve an exchange rate from the canister.");

        assert_eq!(exchange_rate, expected_fiat_crypto_pair_rate);
        assert!(FIAT_CRYPTO_PAIR_BASIC_STD_DEV < exchange_rate.metadata.standard_deviation);

        // Fiat Pair
        let fiat_pair_request = GetExchangeRateRequest {
            timestamp: Some(timestamp_seconds),
            base_asset: eur_asset.clone(),
            quote_asset: Asset {
                symbol: "JPY".to_string(),
                class: AssetClass::FiatCurrency,
            },
        };

        let expected_fiat_pair_rate = ExchangeRate {
            base_asset: eur_asset,
            quote_asset: Asset {
                symbol: "JPY".to_string(),
                class: AssetClass::FiatCurrency,
            },
            timestamp: yesterday_timestamp_seconds,
            rate: 143_229_717_358,
            metadata: ExchangeRateMetadata {
                decimals: 9,
                base_asset_num_queried_sources: NUM_FOREX_SOURCES,
                base_asset_num_received_rates: NUM_FOREX_SOURCES,
                quote_asset_num_queried_sources: NUM_FOREX_SOURCES,
                quote_asset_num_received_rates: NUM_FOREX_SOURCES,
                standard_deviation: 9_438_739_634,
                forex_timestamp: Some(yesterday_timestamp_seconds),
            },
        };

        let fiat_pair_result = container
            .call_canister::<_, GetExchangeRateResult>("get_exchange_rate", &fiat_pair_request)
            .expect("Failed to call canister for rates");
        let exchange_rate =
            fiat_pair_result.expect("Failed to retrieve an exchange rate from the canister.");
        assert_eq!(exchange_rate, expected_fiat_pair_rate);
        assert!(FIAT_PAIR_COMMON_DATASET_STD_DEV < exchange_rate.metadata.standard_deviation);

        Ok(())
    })
    .expect("Scenario failed");
}
