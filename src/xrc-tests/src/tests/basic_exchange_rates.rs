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
///     a. ICP/USDT rates: 
///           GateIo      Okx         Crypto      Mexc        Coinbase    KuCoin      Bitget      Poloniex
///          [ 3900000000, 3900000000, 3910000000, 3911000000, 3920000000, 3920000000, 3930000000, 4005000000, ]
/// 1. The XRC retrieves the BTC/USDT rate.
///     a. BTC/USDT rates: [ 41960000000, 42030000000, 42640000000, 44250000000, 44833000000, 44930000000, 46022000000, 46101000000, ]
/// 2. The XRC divides ICP/USDT by BTC/USDT. The division inverts BTC/USDT to USDT/BTC then multiplies ICP/USDT and USDT/BTC
///    to get the resulting ICP/BTC rate.
///     a. ICP/BTC rates: [ 84596861, 84596861, 84742078, 84742078, 84813776, 84835468, 84959365, 84981094, 85030691, 
///                         85030691, 85176652, 85176652, 85247608, 85393942, 86801691, 86801691, 86874469, 86989492, 
///                         86989492, 87023595, 87024259, 87046516, 87212542, 87234847, 87246828, 87246828, 87435592,
///                         87435592, 87469396, 87658644, 88135593, 88135593, 88361581, 88384180, 88587570, 88587570, 
///                         88813559, 89138660, 89331516, 90508474, 91463412, 91463412, 91697933, 91721386, 91932455, 
///                         91932455, 92166979, 92790863, 92790863, 92945661, 92945661, 93028788, 93052580, 93183984, 
///                         93207816, 93266713, 93266713, 93422306, 93422306, 93925888, 93504639, 93660629, 95289078, 
///                         95448045 ]
/// 3. The XRC returns the median rate and the standard deviation from the BTC/ICP rates.
///     a. The median rate from step 2 is 88248587.
///     b. The standard deviation from step 2 is 3320321.
/// Crypto-fiat pair (retrieve BTC/EUR rate)
/// 0. The XRC retrieves rates from the mock forex sources.
///     a. During collection the rates retrieved are normalized to USD.
///     b. When the collected rates are normalized, then the EUR rate (EUR/USD) is collected (for more information on this collection, see xrc/forex.rs:483).
///         i. For all requests in the following test, this should result in a EUR/USD with the following rates:
///             [917777444, 976400000, 1052938432, 1056100000, 1056900158, 1057200262, 1057421866,
///              1058173944, 1058502845, 1058516154, 1059297297]
/// 1. The XRC retrieves the BTC/USDT rates from the mock exchange responses (request 1 responses).
///     a. BTC/USDT rates: [ 41960000000, 42030000000, 42640000000, 44250000000, 44833000000, 44930000000, 46022000000, 46101000000, ]
/// 2. The XRC retrieves the stablecoin rates from the mock exchanges.
///     a.  DAI:  [ 950000000, 990000000, 990000000, 1000000000, 1020000000, 1030927835 ]
///     b. USDC: [ 950000000, 990000000, 990000000, 1000000000, 1020000000, 1030927835 ]
/// 3. The XRC determines the USDT/USD rate.
///     a. USDT/USD: [ 980392156, 990000000, 990000000, 1010000000, 1030927835, 1030927835, 1052631578 ]
/// 4. The XRC then multiplies the USDT/USD rate (step 3) with the BTC/USDT rate (step 1) to get the BTC/USD rate.
///       [41137254865, 41205882316, 41540400000, 41540400000, 41609700000, 41609700000, 41803921531, 42213600000,
///        42213600000, 42379600000, 42450300000, 43066400000, 43257731956, 43257731956, 43329896905, 43329896905,
///        43382352903, 43807500000, 43807500000, 43953921529, 43958762884, 43958762884, 44049019569, 44168421012,
///        44242105223, 44384670000, 44384670000, 44480700000, 44480700000, 44692500000, 44884210485, 45119607803,
///        45197058783, 45281330000, 45379300000, 45561780000, 45561780000, 45618556698, 45618556698, 45639990000,
///        45639990000, 46219587626, 46219587626, 46319587626, 46319587626, 46482220000, 46562010000, 46578947326,
///        47192631536, 47294736799, 47445360822, 47445360822, 47526804121, 47526804121, 48444210482, 48527368377]
/// 5. The XRC divides the BTC/USD by the forex rate EUR/USD. The division works by inverting EUR/USD to USD/EUR then multiplying
///    USD/EUR and BTC/USD resulting in BTC/EUR.
///     a. This results in the following rates:
///           [38834475422, 38863133731, 38863622360, 38899261248, 38903351774, 38911506495, 38922555303, 38927967367,
///            38928456811, 38952045120, 38968252504, 38976420829, 38987488069, 39017027083, 39069003067, 39134180145,
///            39215053316, 39215053316, 39243992477, 39243992477, 39244485894, 39244485894, 39280474044, 39280474044,
///            39284604657, 39284604657, 39292839294, 39292839294, 39303996380, 39303996380, 39309461483, 39309461483,
///            39309955723, 39309955723, 39333775198, 39333775198, 39350141414, 39350141414, 39358389788, 39358389788,
///            39369565488, 39369565488, 39399393984, 39399393984, 39451879332, 39451879332, 39463823450, 39492946194,
///            39493442741, 39517695146, 39517695146, 39533816007, 39542102882, 39553330746, 39583298473, 39702151829,
///            39850568956, 39850568956, 39879977103, 39879977103, 39880478516, 39880478516, 39921247440, 39921247440,
///            39929615526, 39929615526, 39940953424, 39940953424, 39971214834, 39971214834, 40007276615, 40036800406,
///            40037303791, 40074018974, 40078233034, 40086634027, 40091232953, 40091232953, 40098016509, 40103592018, 
///            40104096243, 40128396919, 40145093766, 40153508774, 40164910245, 40195341337, 40248886995, 40316032422,
///            40655630955, 40685633206, 40686144749, 40727737287, 40736274426, 40747841372, 40778714123, 40836252539,
///            40836252539, 40866388082, 40866388082, 40866901897, 40866901897, 40901156851, 40904377842, 40904377842,
///            40908679220, 40908679220, 40917254286, 40917254286, 40928872621, 40928872621, 40934563658, 40934563658,
///            40935078331, 40935078331, 40953897461, 40959882532, 40959882532, 40976925348, 40976925348, 40984119820,
///            40984635116, 40985514721, 40985514721, 40997152438, 40997152438, 41026532794, 41028214081, 41028214081,
///            41035132565, 41046784371, 41077883618, 41082869238, 41082869238, 41151405961, 41151405961, 41201224636,
///            41355245692, 41355245692, 41385764231, 41385764231, 41386284576, 41386284576, 41428592852, 41428592852,
///            41437276900, 41437276900, 41449042894, 41449042894, 41480446914, 41480446914, 41493470843, 41498041189,
///            41498041189, 41524091387, 41524613471, 41528665105, 41528665105, 41529187247, 41529187247, 41567063158,
///            41571641609, 41571641609, 41575776231, 41580355643, 41580355643, 41583245489, 41587581551, 41592162263,
///            41592162263, 41604996674, 41604996674, 41613932283, 41614455497, 41619090535, 41623674718, 41623674718,
///            41656997027, 41665728952, 41677559814, 41695963084, 41709136970, 41726733058, 41727257691, 41744056589,
///            41748654536, 41748654536, 41765522603, 41769914536, 41778670130, 41790533061, 41796343910, 41796869418,
///            41822195812, 41834373399, 41839597425, 41848367626, 41860250348, 41891965920, 41900106895, 41900106895,
///            41931027520, 41931027520, 41931554721, 41931554721, 41947771713, 41974420414, 41974420414, 41983218876,
///            41983218876, 41990761332, 41990761332, 41995139889, 41995139889, 42017751313, 42021748856, 42021748856,
///            42022277198, 42022277198, 42026957661, 42026957661, 42065235635, 42065235635, 42074053133, 42074053133,
///            42085999937, 42085999937, 42117886550, 42117886550, 42131559633, 42153148382, 42153148382, 42190705201,
///            42201845839, 42221840276, 42222371133, 42244350295, 42244350295, 42265534122, 42274393606, 42286397296,
///            42318435740, 42371684125, 42402952755, 42403485890, 42445501657, 42446834028, 42455731515, 42467786695,
///            42499962570, 42544448956, 42544448956, 42593904382, 42615423966, 42615423966, 42625337002, 42625872933,
///            42627573542, 42667019815, 42669448412, 42678392562, 42690510967, 42698506391, 42699043241, 42722855590,
///            42742693521, 42746573701, 42751653025, 42763792232, 42778118985, 42778656837, 42796192376, 42814339913,
///            42822388504, 42831364712, 42839059541, 42843526553, 42851135823, 42870673076, 42871212091, 42875987108,
///            42915038375, 42924034004, 42924692812, 42936222158, 42968752945, 43004727137, 43011324683, 43011324683,
///            43043065343, 43043065343, 43043606526, 43043606526, 43064923138, 43064923138, 43085156647, 43085156647,
///            43087609045, 43087609045, 43096640848, 43096640848, 43096703351, 43096703351, 43097245208, 43097245208,
///            43097771513, 43108878013, 43108878013, 43116951792, 43116951792, 43117493904, 43117493904, 43141302561,
///            43141302561, 43141539613, 43141539613, 43150345619, 43150345619, 43161571957, 43161571957, 43162598033,
///            43162598033, 43170619263, 43170619263, 43182877434, 43182877434, 43195300334, 43195300334, 43215595100,
///            43215595100, 43233920483, 43233920483, 43271076993, 43271076993, 43324999137, 43324999137, 43345354840,
///            43345354840, 43403932773, 43476341622, 43632309583, 43632309583, 43664508505, 43664508505, 43665057501,
///            43665057501, 43709695316, 43709695316, 43718857517, 43718857517, 43726711787, 43726711787, 43731271359,
///            43731271359, 43758980374, 43758980374, 43759530559, 43759530559, 43764404517, 43764404517, 43804264951,
///            43804264951, 43813446975, 43813446975, 43825887676, 43825887676, 43859092520, 43859092520, 43880240334,
///            43895812120, 43895812120, 43912622219, 43913174334, 43955563852, 43958065793, 43967280057, 43971553062,
///            43979764437, 43988001323, 43988554387, 43990784435, 43990784435, 44004002332, 44004555597, 44013085868,
///            44033522905, 44042752986, 44049540473, 44055258796, 44058773910, 44071284270, 44088637425, 44104675041, 
///            44107333018, 44145240164, 44221018574, 44237104345, 44303289548, 44303289548, 44377198755, 44377198755, 
///            44430922636, 44550884484, 44583761278, 44584321832, 44629899390, 44639254480, 44647274103, 44651929665, 
///            44680222029, 44680783796, 44685760364, 44726459965, 44735835295, 44748537904, 44782441799, 44789466501, 
///            44789466501, 44819934443, 44822519359, 44822519359, 44822691052, 44823082915, 44823082915, 44866345717, 
///            44866345717, 44866350771, 44866350771, 44868904553, 44868904553, 44878309742, 44878309742, 44891052807, 
///            44891052807, 44897466753, 44899460366, 44899460366, 44900024890, 44900024890, 44916906174, 44925064678, 
///            44925064678, 44945925184, 44945925184, 44955346517, 44955346517, 44968111456, 44968111456, 45002181711, 
///            45002181711, 45016306316, 45021264689, 45021264689, 45059957295, 45059957295, 45113702916, 45137305881, 
///            45137305881, 45235990342, 45261953465, 45261953465, 45311455532, 45337461967, 45337461967, 45457466159, 
///            45457466159, 45549083567, 45555817245, 45555817245, 45732402599, 45766151306, 45766726727, 45772736540, 
///            45810905485, 45813513031, 45823116224, 45836127564, 45844712124, 45845288532, 45870855474, 45892155149, 
///            45901774826, 45914808501, 45949596024, 45969080748, 45995464627, 45995464627, 46008587936, 46087564913, 
///            46176336364, 46210167718, 46253370290, 46289490721, 46375798809, 46476136785, 46663027404, 46663027404, 
///            46721176418, 46721176418, 46743127772, 46743127772, 46924665933, 47133139084, 47133139084, 47211769202, 
///            47211769202, 47268924669, 47336734517, 47336734517, 47439151559, 47439151559, 47605714826, 47687433384, 
///            47704780092, 47732160173, 47732160173, 47891699427, 47896974513, 47896974513, 47995317184, 48125415655, 
///            48205701144, 48333297308, 48361038125, 48361038125, 48437870498, 48465671335, 48465671335, 48592135167, 
///            48592135167, 48675546985, 48675546985, 48696446237, 48905331829, 49161818104, 49246207822, 49338028794, 
///            49444775806, 49615127444, 49643603965, 49643603965, 49700295300, 49705467218, 49705467218, 49728820703, 
///            49728820703, 50360343769, 50360343769, 50469302647, 50469302647, 50646505056, 50733443344, 50751898064, 
///            51420561489, 51531814238, 51695932482, 51695932482, 51784672186, 51784672186, 52784267858, 52874875766]
/// 6. The XRC then returns the median and the standard deviation.
///     a. The median rate from step 5 is 42895512741.
///     b. The standard deviation from step 5 is 2810207067.
/// Fiat-crypto pair (retrieve EUR/BTC rate)
/// 0. The instructions are similar to the crypto-fiat pair. The only difference is that the rates are inverted before
///    being returned.
///     a. When inverted, the median rate is 22985171.
///     b. When inverted, the standard deviation is 23312466.
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
            xrc::Exchange::CryptoCom(_) => Some("3.91"),
            xrc::Exchange::Bitget(_) => Some("3.93"),
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
        assert_eq!(exchange_rate.metadata.base_asset_num_queried_sources, 8);
        assert_eq!(exchange_rate.metadata.base_asset_num_received_rates, 8);
        assert_eq!(exchange_rate.metadata.quote_asset_num_queried_sources, 8);
        assert_eq!(exchange_rate.metadata.quote_asset_num_received_rates, 8);
        assert_eq!(exchange_rate.metadata.standard_deviation, 3_320_321);
        assert_eq!(exchange_rate.rate, 88_248_587);

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
        assert_eq!(exchange_rate.metadata.standard_deviation, 2_810_207_067);
        assert_eq!(exchange_rate.rate, 42_895_512_741);

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
        assert_eq!(exchange_rate.metadata.standard_deviation, 1_447_565);
        assert_eq!(exchange_rate.rate, 23_312_466);

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
