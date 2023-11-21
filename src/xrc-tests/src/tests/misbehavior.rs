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
///     a. ICP/USDT rates: [3900000000, 3900000000, 3911000000, 4005000000]
///         i. There are only 4 rates as the other 3 have been filtered out as they were >= 20% different
///            than the median rate.
/// 1. The XRC retrieves the BTC/USDT rate.
///     a. BTC/USDT rates: [42030000000, 42640000000, 46022000000, 46101000000]
///         i. There are only 4 rates as the other 3 have been filtered out as they were >= 20% different
///            than the median rate.
/// 2. The XRC divides ICP/USDT by BTC/USDT. The division inverts BTC/USDT to USDT/BTC then multiplies ICP/USDT and USDT/BTC
///    to get the resulting ICP/BTC rate.
//     a. ICP/BTC rates: [84596861, 84596861, 84742078, 84742078, 84835468, 84981094, 86874469,
///                       87023595, 91463412, 91463412, 91721386, 92790863, 92790863, 93052580,
///                       93925888, 95289078]
/// 3. The XRC returns the median rate and the standard deviation from the BTC/ICP rates.
///     a. The median rate from step 2 is 89243503.
///     b. The standard deviation from step 2 is 4044987.
/// Crypto-fiat pair (retrieve BTC/EUR rate)
/// 0. The XRC retrieves rates from the mock forex sources.
///     a. During collection the rates retrieved are normalized to USD.
///     b. When the collected rates are normalized, then the EUR rate (EUR/USD) is collected (for more information on this collection, see xrc/forex.rs:483).
///         i. For all requests in the following test, this should result in a EUR/USD with the following rates:
///             [917777444, 976400000, 1056100000, 1056900158, 1057200262, 1058516154]
///         ii. Large values are filtered out as they were greater than the median rate.
/// 1. The XRC retrieves the BTC/USDT rates from the mock exchange responses (request 1 responses).
///     a. BTC/USDT rates: [42030000000, 42640000000, 46022000000, 46101000000]
///         i. There are only 4 rates as the other 3 have been filtered out as they were greater
///            than the median rate.
/// 2. The XRC retrieves the stablecoin rates from the mock exchanges.
///     a.  DAI:  [ 950000000, 990000000, 990000000, 1000000000, 1020000000, 1030927835 ]
///     b. USDC: [ 950000000, 990000000, 990000000, 1000000000, 1020000000, 1030927835 ]
/// 3. The XRC determines the USDT/USD rate.
///     a. USDT/USD: [ 970000000, 980392156, 1000000000, 1010101010, 1010101010, 1052631578 ]
/// 4. The XRC then multiplies the USDT/USD rate (step 3) with the ICP/USDT rate (step 1) to get the BTC/USD rate.
///     a. This results in the following rates:
///        [ 40701200000, 40769100000, 41137254865, 41205882316, 41360800000, 41803921531, 41960000000,
///          42030000000, 42383838379, 42383838379, 42454545450, 42454545450, 42640000000, 42922500000,
///          43070707066, 43070707066, 43382352903, 43488010000, 43953921529, 44168421012, 44242105223,
///          44250000000, 44641340000, 44696969692, 44696969692, 44717970000, 44833000000, 44884210485,
///          45119607803, 45197058783, 45285858581, 45285858581, 46022000000, 46101000000, 46486868682,
///          46486868682, 46566666662, 46566666662, 46578947326, 47192631536, 48444210482, 48527368377]
/// 5. The XRC divides the BTC/USD by the forex rate EUR/USD. The division works by inverting EUR/USD to USD/EUR then multiplying
///    USD/EUR and BTC/USD resulting in BTC/EUR.
///     a. This results in the following rates:
///         [ 38422830017, 38451184548, 38451667997, 38463619538, 38486929114, 38490976280, 38499044560, 38509976251, 38515330948, 38515815203,
///           38527786682, 38539153477, 38555189062, 38563270803, 38574220730, 38603446631, 38654871669, 38719357870, 38834475422, 38863133731,
///           38863622360, 38875701944, 38899261248, 38903351774, 38911506495, 38922555303, 38927967367, 38928456811, 38940556547, 38952045120,
///           38968252504, 38976420829, 38987488069, 39017027083, 39045506957, 39069003067, 39074321000, 39074812283, 39086957510, 39114757593,
///           39122956627, 39134065476, 39134180145, 39163715544, 39281309055, 39463823450, 39492946194, 39493442741, 39505718086, 39533816007,
///           39542102882, 39553330746, 39583298473, 39611164966, 39640396442, 39640894843, 39653216019, 39677246509, 39681418846, 39689736660,
///           39701006445, 39702151829, 39706526750, 39707025983, 39719367714, 39731086058, 39747617590, 39755949281, 39767237866, 39797367660,
///           39850383164, 39916863784, 40011277739, 40011277739, 40040804482, 40040804482, 40041307917, 40041307917, 40053753550, 40053753550,
///           40078026772, 40078026772, 40082241254, 40082241254, 40090643087, 40090643087, 40102026707, 40102026707, 40107602774, 40107602774,
///           40108107049, 40108107049, 40120573444, 40120573444, 40132410155, 40132410155, 40149108672, 40149108672, 40157524522, 40157524522,
///           40168927134, 40168927134, 40199361269, 40199361269, 40252912282, 40252912282, 40253099956, 40282805154, 40283311632, 40295832484,
///           40320064424, 40320064424, 40324492364, 40332944976, 40344397398, 40374964479, 40496194902, 40519786183, 40549688186, 40550198019,
///           40562801825, 40591651582, 40600160195, 40611688492, 40642458087, 40659696920, 40659696920, 40689702172, 40689702172, 40690213765,
///           40690213765, 40702861091, 40702861091, 40731810464, 40731810464, 40740348456, 40740348456, 40751916559, 40751916559, 40764491691,
///           40782792398, 40782792398, 40905247372, 40905247372, 40953897461, 40984119820, 40984635116, 40997373953, 41026532794, 41035132565,
///           41046784371, 41053640089, 41077883618, 41083936055, 41084452605, 41097222468, 41126452325, 41135073040, 41146753224, 41177928213,
///           41201224636, 41301569626, 41493470843, 41524091387, 41524613471, 41537520145, 41567063158, 41575776231, 41587581551, 41619090535,
///           41684965139, 41695963084, 41726733058, 41727257691, 41740227350, 41744056589, 41754506310, 41765522603, 41769914536, 41772975447,
///           41778670130, 41790533061, 41796343910, 41796869418, 41803802253, 41804327855, 41809860714, 41817321469, 41822195812, 41839597425,
///           41847063487, 41848367626, 41855835253, 41860250348, 41867720095, 41891965920, 41899441327, 41947771713, 42017751313, 42025249166,
///           42131559633, 42142409033, 42173508467, 42174038717, 42187147245, 42194924689, 42194924689, 42201845839, 42214749442, 42217152297,
///           42226001639, 42226062877, 42226062877, 42226593788, 42226593788, 42237991588, 42239718651, 42239718651, 42245902261, 42246433421,
///           42259564450, 42269761093, 42269761093, 42269993358, 42278621463, 42278621463, 42289621008, 42290626354, 42290626354, 42298485541,
///           42310496072, 42322668002, 42322668002, 42323340298, 42342552775, 42354573252, 42355105779, 42360507948, 42368270586, 42371684125,
///           42396913821, 42398404459, 42402952755, 42403485890, 42407291794, 42416665734, 42419333221, 42446834028, 42449746627, 42449746627,
///           42451472384, 42455731515, 42467786695, 42469691106, 42499962570, 42578937759, 42593904382, 42625337002, 42625872933, 42627573542,
///           42639121900, 42667019815, 42669448412, 42678392562, 42690510967, 42698506391, 42699043241, 42712314951, 42722855590, 42742693521,
///           42750848781, 42750848781, 42751653025, 42763792232, 42782397220, 42782397220, 42782935126, 42782935126, 42796192376, 42796232910,
///           42796232910, 42814339913, 42826671166, 42826671166, 42835648272, 42835648272, 42847811330, 42847811330, 42851135823, 42880275131,
///           42880275131, 42924692812, 42974190865, 43009028035, 43009028035, 43045882794, 43408273596, 43408273596, 43445782508, 43477843781,
///           43478390430, 43480689686, 43480689686, 43491904376, 43520360250, 43522837419, 43531960452, 43544321225, 43552476558, 43553024145,
///           43566561289, 43577312740, 43597547431, 43606686124, 43619068115, 43652116263, 43670626751, 43708158579, 43783186707, 43884628792,
///           43884628792, 43917013915, 43917013915, 43917566086, 43917566086, 43931216537, 43931216537, 43959954895, 43959959844, 43959959844,
///           43962462035, 43962462035, 43971553062, 43971677220, 43971677220, 43984162849, 43984162849, 43992400559, 43992400559, 43992953678,
///           43992953678, 44004002332, 44004555597, 44006627560, 44006627560, 44017487612, 44017487612, 44018233085, 44037926693, 44037926693,
///           44047157697, 44047157697, 44049540473, 44058773910, 44059664758, 44059664758, 44071284270, 44093046725, 44093046725, 44104675041,
///           44111744188, 44111744188, 44149655125, 44149655125, 44225441114, 44225441114, 44237104345, 44347570567, 44421553645, 44430922636,
///           44539133510, 44550884484, 44583761278, 44584321832, 44598179523, 44629899390, 44639254480, 44651929665, 44685760364, 44819934443,
///           44822691052, 44897466753, 45016306316, 45066263322, 45235990342, 45311455532, 45319541129, 45549083567, 45719144915, 45720339981,
///           45732402599, 45766151306, 45766726727, 45777314266, 45777314266, 45780951933, 45795416129, 45798822161, 45810905485, 45813513031,
///           45823116224, 45836127564, 45844712124, 45845288532, 45859538157, 45870855474, 45892155149, 45901774826, 45914808501, 45916632484,
///           45949596024, 45969080748, 46008587936, 46087564913, 46180954454, 46180954454, 46210167718, 46257996085, 46257996085, 46289490721,
///           46380436848, 46380436848, 46460065280, 46767874109, 46929358864, 46929358864, 47134371115, 47215280578, 47268924669, 47384047456,
///           47610475869, 47610475869, 47692202599, 47692202599, 47704780092, 47891699427, 48125415655, 48205701144, 48214303205, 48333297308,
///           48640702875, 48701316363, 48701316363, 48724198063, 48849533459, 48905331829, 49161818104, 49246207822, 49342963085, 49342963085,
///           49615127444, 49700295300, 50145054511, 50231132024, 50651570207, 50651570207, 50738517190, 50738517190, 50751898064, 51420561489,
///           52784267858, 52874875766]
/// 6. The XRC then returns the median and the standard deviation.
///     a. The median rate from step 5 is 42316582037.
///     b. The standard deviation from step 5 is 3304591113.
/// Fiat-crypto pair (retrieve EUR/BTC rate)
/// 0. The instructions are similar to the crypto-fiat pair. The only difference is that the rates are inverted before
///    being returned.
///     a. When inverted, the median rate is 42316582037.
///     b. When inverted, the standard deviation is 1692069.
/// Fiat pair (retrieve EUR/JPY rate)
/// 0. The XRC retrieves rates from the mock forex sources.
///     a. During collection the rates retrieved are normalized to USD.
///     b. When the collected rates are normalized, then the EUR rate (EUR/USD) is collected (for more information on this collection, see xrc/forex.rs:483).
///         i. For all requests in the following test, this should result in a EUR/USD with the following rates:
///             [917777444, 976400000, 1056100000, 1056900158, 1057200262, 1058516154]
///         ii. Large values are filtered out as they were greater than the median rate.
///         i. For all requests in the following test, this should result in a EUR/USD with the following rates:
///             [6900840, 7217610, 7350873, 7380104, 7395293, 7395930]
///         ii. Large values are filtered out as they were greater than the median rate.
/// 1. The XRC divides EUR/USD by JPY/USD. The division inverts JPY/USD to USD/JPY then multiplies EUR/USD and USD/JPY
///    to get the resulting EUR/JPY rate.
///     a. EUR/JPY rates should then include:
///       [ 124092229644, 124102918437, 124358334787, 124852849994, 127158081968, 132018556151, 132029927684, 132301658621, 132827760729,
///         132995033068, 135280238194, 141490021504, 142794753330, 142807053080, 142902942293, 142915251362, 142943519205, 142955831769,
///         143100964430, 143121440305, 143133768195, 143209385395, 143250049321, 143428351957, 143670010350, 143778862455, 143819688082,
///         143998699745, 146322674679, 146433536585, 146475115999, 146657432861, 153039340138, 153155290949, 153198778988, 153389464760 ]
/// 2. The XRC then return the median and the standard deviation.
///     a. The median rate from the group of rates in step 1.a.: 142800903205.
///     b. The standard deviation of the group of rates in step 1.a.: 17672086919.
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
            xrc::Exchange::Bybit(_) => Some("100000.0"),
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
            xrc::Exchange::Bybit(_) => Some("10000.96000000"),
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
            rate: 89243503,
            metadata: ExchangeRateMetadata {
                decimals: 9,
                base_asset_num_queried_sources: NUM_EXCHANGES,
                base_asset_num_received_rates: NUM_EXCHANGES,
                quote_asset_num_queried_sources: NUM_EXCHANGES,
                quote_asset_num_received_rates: NUM_EXCHANGES,
                standard_deviation: 4_044_987,
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
            rate: 43506325069,
            metadata: ExchangeRateMetadata {
                decimals: 9,
                base_asset_num_queried_sources: NUM_EXCHANGES,
                base_asset_num_received_rates: NUM_EXCHANGES,
                quote_asset_num_queried_sources: NUM_FOREX_SOURCES,
                quote_asset_num_received_rates: NUM_FOREX_SOURCES,
                standard_deviation: 3_304_591_113,
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
            rate: 22985171,
            metadata: ExchangeRateMetadata {
                decimals: 9,
                base_asset_num_queried_sources: NUM_FOREX_SOURCES,
                base_asset_num_received_rates: NUM_FOREX_SOURCES,
                quote_asset_num_queried_sources: NUM_EXCHANGES,
                quote_asset_num_received_rates: NUM_EXCHANGES,
                standard_deviation: 1_692_069,
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
            rate: 142_800_903_205,
            metadata: ExchangeRateMetadata {
                decimals: 9,
                base_asset_num_queried_sources: NUM_FOREX_SOURCES,
                base_asset_num_received_rates: NUM_FOREX_SOURCES,
                quote_asset_num_queried_sources: NUM_FOREX_SOURCES,
                quote_asset_num_received_rates: NUM_FOREX_SOURCES,
                standard_deviation: 17_672_086_919,
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
