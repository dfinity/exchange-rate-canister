use ic_xrc_types::{Asset, AssetClass, GetExchangeRateRequest, GetExchangeRateResult};

use crate::tests::{NUM_EXCHANGES, NUM_FOREX_SOURCES};
use crate::{
    container::{run_scenario, Container},
    mock_responses, ONE_DAY_SECONDS,
};

/// Setup:
/// * Deploy mock FOREX data providers and exchanges.
/// * Start replicas and deploy the XRC, configured to use the mock data sources
///
/// Runbook:
/// * Request exchange rate for various cryptocurrency and fiat currency pairs
/// * Assert that the returned rates correspond to the expected values
///
/// Success criteria:
/// * All queries return the expected values
///
///
/// The expected values are determined as follows:
///
/// Crypto-pair (retrieve ICP/BTC rate)
/// 0. The XRC retrieves the ICP/USDT rate.
///     a. ICP/USDT rates: [ 3900000000, 3900000000, 3910000000, 3911000000, 3920000000, 3920000000, 4005000000, ]
/// 1. The XRC retrieves the BTC/USDT rate.
///     a. BTC/USDT rates: [ 41960000000, 42030000000, 42640000000, 44250000000, 44833000000, 46022000000, 46101000000, ]
/// 2. The XRC divides ICP/USDT by BTC/USDT. The division inverts BTC/USDT to USDT/BTC then multiplies ICP/USDT and USDT/BTC
///    to get the resulting ICP/BTC rate.
///     a. ICP/BTC rates: [ 84596861, 84596861, 84742078, 84742078, 84813776, 84835468, 84959365, 84981094,
///                         85030691, 85030691, 85176652, 85176652, 86874469, 86989492, 86989492, 87023595,
///                         87212542, 87234847, 87435592, 87435592, 88135593, 88135593, 88361581, 88384180,
///                         88587570, 88587570, 89331516, 90508474, 91463412, 91463412, 91697933, 91721386,
///                         91932455, 91932455, 92790863, 92790863, 92945661, 92945661, 93028788, 93052580,
///                         93183984, 93207816, 93266713, 93266713, 93422306, 93422306, 93925888, 95289078,
///                         95448045 ]
/// 3. The XRC returns the median rate and the standard deviation from the BTC/ICP rates.
///     a. The median rate from step 2 is 88587570.
///     b. The standard deviation from step 2 is 3483761.
/// Crypto-fiat pair (retrieve BTC/EUR rate)
/// 0. The XRC retrieves rates from the mock forex sources.
///     a. During collection the rates retrieved are normalized to USD.
///     b. When the collected rates are normalized, then the EUR rate (EUR/USD) is collected (for more information on this collection, see xrc/forex.rs:483).
///         i. For all requests in the following test, this should result in a EUR/USD with the following rates:
///             [917777444, 976400000, 1052938432, 1056100000, 1056900158, 1057200262, 1057421866,
///              1058173944, 1058502845, 1058516154, 1059297297]
/// 1. The XRC retrieves the BTC/USDT rates from the mock exchange responses (request 1 responses).
///     a. BTC/USDT rates: [ 41960000000, 42030000000, 42640000000, 44250000000, 44833000000, 46022000000, 46101000000, ]
/// 2. The XRC retrieves the stablecoin rates from the mock exchanges.
///     a.  DAI:  [ 950000000, 990000000, 990000000, 1000000000, 1020000000, 1030927835 ]
///     b. USDC: [ 950000000, 990000000, 990000000, 1000000000, 1020000000, 1030927835 ]
/// 3. The XRC determines the USDT/USD rate.
///     a. USDT/USD: [ 970000000, 980392156, 1000000000, 1010101010, 1010101010, 1052631578 ]
/// 4. The XRC then multiplies the USDT/USD rate (step 3) with the BTC/USDT rate (step 1) to get the BTC/USD rate.
///     a. This results in the following rates:
///         [ 40769100000, 41205882316, 41360800000, 41803921531, 42030000000, 42454545450, 42454545450,
///          42640000000, 43070707066, 43070707066, 44242105223, 44641340000, 44717970000, 44884210485,
///          45119607803, 45197058783, 46022000000, 46101000000, 46486868682, 46486868682, 46566666662,
///          46566666662, 48444210482, 48527368377 ]
/// 5. The XRC divides the BTC/USD by the forex rate EUR/USD. The division works by inverting EUR/USD to USD/EUR then multiplying
///    USD/EUR and BTC/USD resulting in BTC/EUR.
///     a. This results in the following rates:
///         [ 38515330948, 38563270803, 38574220730, 38603446631, 38927967367, 38976420829, 38987488069, 39017027083, 39074321000, 39122956627,
///           39134065476, 39163715544, 39492946194, 39542102882, 39553330746, 39583298473, 39706526750, 39755949281, 39767237866, 39797367660,
///           40107602774, 40107602774, 40157524522, 40157524522, 40168927134, 40168927134, 40199361269, 40199361269, 40282805154, 40332944976,
///           40344397398, 40374964479, 40689702172, 40689702172, 40740348456, 40740348456, 40751916559, 40751916559, 40782792398, 40782792398,
///           41754506310, 41796343910, 41848367626, 41860250348, 41891965920, 42173508467, 42201845839, 42226001639, 42237991588, 42245902261,
///           42269993358, 42298485541, 42310496072, 42342552775, 42360507948, 42402952755, 42455731515, 42467786695, 42499962570, 42625337002,
///           42678392562, 42690510967, 42698506391, 42722855590, 42751653025, 42763792232, 42796192376, 42814339913, 43045882794, 43477843781,
///           43480689686, 43480689686, 43531960452, 43544321225, 43552476558, 43577312740, 43606686124, 43619068115, 43652116263, 43670626751,
///           43917013915, 43917013915, 43971677220, 43971677220, 43984162849, 43984162849, 43992400559, 43992400559, 44017487612, 44017487612,
///           44047157697, 44047157697, 44059664758, 44059664758, 44093046725, 44093046725, 44111744188, 44111744188, 44421553645, 44897466753,
///           45066263322, 45311455532, 45549083567, 45720339981, 45766151306, 45795416129, 45798822161, 45823116224, 45836127564, 45844712124,
///           45870855474, 45901774826, 45914808501, 45949596024, 45969080748, 46210167718, 46257996085, 46257996085, 46289490721, 46460065280,
///           46929358864, 46929358864, 47134371115, 47215280578, 47610475869, 47610475869, 47692202599, 47692202599, 48205701144, 48640702875,
///           48724198063, 48905331829, 49161818104, 49246207822, 49615127444, 49700295300, 50145054511, 50231132024, 50651570207, 50651570207,
///           50738517190, 50738517190, 52784267858, 52874875766]
/// 6. The XRC then returns the median and the standard deviation.
///     a. The median rate from step 5 is 43506325069.
///     b. The standard deviation from step 5 is 2851294695.
/// Fiat-crypto pair (retrieve EUR/BTC rate)
/// 0. The instructions are similar to the crypto-fiat pair. The only difference is that the rates are inverted before
///    being returned.
///     a. When inverted, the median rate is 22985171.
///     b. When inverted, the standard deviation is 23610052.
/// Fiat pair (retrieve EUR/JPY rate)
/// 0. The XRC retrieves rates from the mock forex sources.
///     a. During collection the rates retrieved are normalized to USD.
///     b. When the collected rates are normalized, then the EUR rate (EUR/USD) is collected (for more information on this collection, see xrc/forex.rs:483).
///         i. For all requests in the following test, this should result in a EUR/USD with the following rates:
///             [917777444, 976400000, 1052938432, 1056100000, 1056900158, 1057200262, 1057421866,
///              1058173944, 1058502845, 1058516154, 1059297297]
///         ii. For all requests in the following test, this should result in a JPY/USD with the following rates:
///             [6900840, 7346082, 7350873, 7369729, 7380104, 7390111, 7395293, 7395822, 7395930, 7399602]
/// 1. The XRC divides EUR/USD by JPY/USD. The division inverts JPY/USD to USD/JPY then multiplies EUR/USD and USD/JPY
///    to get the resulting EUR/JPY rate.
///     a. EUR/JPY rates should then include:
///       [124030649755, 124092229644, 124094041743, 124102918437, 124189940312, 124358334787, 124533404687, 124852849994, 124934277074,
///        131953042878, 132018556151, 132020483996, 132029927684, 132122508037, 132301658621, 132487911020, 132827760729, 132914388921,
///        132995033068, 141490021504, 142296630547, 142367279300, 142369358266, 142379542230, 142479379808, 142672573719, 142723892446,
///        142794753330, 142796838538, 142807053080, 142832027721, 142872584497, 142873426145, 142902532594, 142902942293, 142905029081,
///        142907190432, 142915251362, 142943519205, 142945606586, 142955831769, 142973482171, 142975569989, 142985797316, 143004170223,
///        143015464584, 143048618694, 143050417305, 143056073446, 143075170262, 143077259565, 143086060005, 143087494166, 143100964430,
///        143119640802, 143121440305, 143121730754, 143123530284, 143131968536, 143133768195, 143155982847, 143187828165, 143209385395,
///        143227058260, 143229149781, 143232333722, 143234134642, 143239395247, 143239916129, 143250049321, 143280076540, 143302419939,
///        143333334966, 143339835761, 143381982692, 143410993537, 143426548595, 143428351957, 143451714709, 143481784200, 143534196401,
///        143583833814, 143628462457, 143630268357, 143670010350, 143736261807, 143763709688, 143778862455, 143819688082, 143849834706,
///        143872632785, 143913485038, 143943651322, 143952146091, 143996889212, 143998699745, 144046029434, 144090801735, 144092613449,
///        144104965083, 144198948091, 152581197651, 153039340138, 153155290949, 153198778988, 153230891601, 153339875145, 153387536154,
///        153389464760, 153502660110]
/// 2. The XRC then return the median and the standard deviation.
///     a. The median rate from the group of rates in step 1.a.: 143121585529.
///     b. The standard deviation of the group of rates in step 1.a.: 7028947458.
#[ignore]
#[test]
fn basic_exchange_rates() {
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
            xrc::Exchange::Coinbase(_) => Some("3.92"),
            xrc::Exchange::KuCoin(_) => Some("3.92"),
            xrc::Exchange::Okx(_) => Some("3.90"),
            xrc::Exchange::GateIo(_) => Some("3.90"),
            xrc::Exchange::Mexc(_) => Some("3.911"),
            xrc::Exchange::Poloniex(_) => Some("4.005"),
            xrc::Exchange::Bybit(_) => Some("3.91"),
        },
    )
    .chain(mock_responses::exchanges::build_common_responses(
        "BTC".to_string(),
        timestamp_seconds,
    ))
    .chain(mock_responses::stablecoin::build_responses(
        timestamp_seconds,
    ))
    .chain(mock_responses::forex::build_common_responses(now_seconds))
    .collect::<Vec<_>>();

    let container = Container::builder()
        .name("basic_exchange_rates")
        .exchange_responses(responses)
        .build();

    run_scenario(container, |container: &Container| {
        // Crypto pair
        let crypto_pair_request = GetExchangeRateRequest {
            timestamp: Some(timestamp_seconds),
            base_asset: Asset {
                symbol: "ICP".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            quote_asset: Asset {
                symbol: "BTC".to_string(),
                class: AssetClass::Cryptocurrency,
            },
        };
        let crypto_pair_result = container
            .call_canister::<_, GetExchangeRateResult>("get_exchange_rate", &crypto_pair_request)
            .expect("Failed to call canister for rates");
        let exchange_rate =
            crypto_pair_result.expect("Failed to retrieve an exchange rate from the canister.");
        assert_eq!(exchange_rate.base_asset, crypto_pair_request.base_asset);
        assert_eq!(exchange_rate.quote_asset, crypto_pair_request.quote_asset);
        assert_eq!(exchange_rate.timestamp, timestamp_seconds);
        assert_eq!(exchange_rate.metadata.base_asset_num_queried_sources, 7);
        assert_eq!(exchange_rate.metadata.base_asset_num_received_rates, 7);
        assert_eq!(exchange_rate.metadata.quote_asset_num_queried_sources, 7);
        assert_eq!(exchange_rate.metadata.quote_asset_num_received_rates, 7);
        assert_eq!(exchange_rate.metadata.standard_deviation, 3_483_761);
        assert_eq!(exchange_rate.rate, 88_587_570);

        // Crypto-fiat pair
        let crypto_fiat_pair_request = GetExchangeRateRequest {
            timestamp: Some(timestamp_seconds),
            base_asset: Asset {
                symbol: "BTC".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            quote_asset: Asset {
                symbol: "EUR".to_string(),
                class: AssetClass::FiatCurrency,
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
        assert_eq!(
            exchange_rate.base_asset,
            crypto_fiat_pair_request.base_asset
        );
        assert_eq!(
            exchange_rate.quote_asset,
            crypto_fiat_pair_request.quote_asset
        );
        assert_eq!(exchange_rate.timestamp, timestamp_seconds);
        assert_eq!(
            exchange_rate.metadata.base_asset_num_queried_sources,
            NUM_EXCHANGES
        );
        assert_eq!(
            exchange_rate.metadata.base_asset_num_received_rates,
            NUM_EXCHANGES
        );
        assert_eq!(
            exchange_rate.metadata.quote_asset_num_queried_sources,
            NUM_FOREX_SOURCES
        );
        assert_eq!(
            exchange_rate.metadata.quote_asset_num_received_rates,
            NUM_FOREX_SOURCES
        );
        assert_eq!(exchange_rate.metadata.standard_deviation, 2_851_294_695);
        assert_eq!(exchange_rate.rate, 42_354_839_515);

        // Fiat-crypto pair
        let fiat_crypto_pair_request = GetExchangeRateRequest {
            timestamp: Some(timestamp_seconds),
            base_asset: Asset {
                symbol: "EUR".to_string(),
                class: AssetClass::FiatCurrency,
            },
            quote_asset: Asset {
                symbol: "BTC".to_string(),
                class: AssetClass::Cryptocurrency,
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
        assert_eq!(
            exchange_rate.base_asset,
            fiat_crypto_pair_request.base_asset
        );
        assert_eq!(
            exchange_rate.quote_asset,
            fiat_crypto_pair_request.quote_asset
        );
        assert_eq!(exchange_rate.timestamp, timestamp_seconds);
        assert_eq!(
            exchange_rate.metadata.base_asset_num_queried_sources,
            NUM_FOREX_SOURCES
        );
        assert_eq!(
            exchange_rate.metadata.base_asset_num_received_rates,
            NUM_FOREX_SOURCES
        );
        assert_eq!(
            exchange_rate.metadata.quote_asset_num_queried_sources,
            NUM_EXCHANGES
        );
        assert_eq!(
            exchange_rate.metadata.quote_asset_num_received_rates,
            NUM_EXCHANGES
        );
        assert_eq!(exchange_rate.metadata.standard_deviation, 1_498_851);
        assert_eq!(exchange_rate.rate, 23_610_052);

        // Fiat-pair
        let fiat_pair_request = GetExchangeRateRequest {
            timestamp: Some(timestamp_seconds),
            base_asset: Asset {
                symbol: "EUR".to_string(),
                class: AssetClass::FiatCurrency,
            },
            quote_asset: Asset {
                symbol: "JPY".to_string(),
                class: AssetClass::FiatCurrency,
            },
        };

        let fiat_pair_result = container
            .call_canister::<_, GetExchangeRateResult>("get_exchange_rate", &fiat_pair_request)
            .expect("Failed to call canister for rates");
        let exchange_rate =
            fiat_pair_result.expect("Failed to retrieve an exchange rate from the canister.");
        assert_eq!(exchange_rate.base_asset, fiat_pair_request.base_asset);
        assert_eq!(exchange_rate.quote_asset, fiat_pair_request.quote_asset);
        assert_eq!(exchange_rate.timestamp, yesterday_timestamp_seconds);
        assert_eq!(
            exchange_rate.metadata.base_asset_num_queried_sources,
            NUM_FOREX_SOURCES
        );
        assert_eq!(
            exchange_rate.metadata.base_asset_num_received_rates,
            NUM_FOREX_SOURCES
        );
        assert_eq!(
            exchange_rate.metadata.quote_asset_num_queried_sources,
            NUM_FOREX_SOURCES
        );
        assert_eq!(
            exchange_rate.metadata.quote_asset_num_received_rates,
            NUM_FOREX_SOURCES
        );
        assert_eq!(exchange_rate.metadata.standard_deviation, 7_028_947_458);
        assert_eq!(exchange_rate.rate, 143_121_585_529);

        Ok(())
    })
    .expect("Scenario failed");
}
