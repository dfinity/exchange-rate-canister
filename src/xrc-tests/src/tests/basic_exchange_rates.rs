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
///           GateIo      Okx         Crypto      Mexc        Coinbase    KuCoin      Bitget      Digifinex   Poloniex
///          [ 3900000000, 3900000000, 3910000000, 3911000000, 3920000000, 3920000000, 3930000000, 4000000000, 4005000000, ]
/// 1. The XRC retrieves the BTC/USDT rate.
///     a. BTC/USDT rates: [ 41960000000, 42030000000, 42640000000, 44250000000, 44250000000, 44833000000, 44930000000, 46022000000, 46101000000, ]
/// 2. The XRC divides ICP/USDT by BTC/USDT. The division inverts BTC/USDT to USDT/BTC then multiplies ICP/USDT and USDT/BTC
///    to get the resulting ICP/BTC rate.
///     a. ICP/BTC rates: [ 84596861, 84596861, 84742078, 84742078, 84813776, 84835468, 84959365, 84981094, 85030691,
///                         85030691, 85176652, 85176652, 85247606, 85393940, 86766012, 86801687, 86801687, 86874469,
///                         86914952, 86989492, 86989492, 87023595, 87024256, 87046512, 87212542, 87234847, 87246824,
///                         87246824, 87435592, 87435592, 87469392, 87658642, 88135593, 88135593, 88361581, 88384180,
///                         88587570, 88587570, 88636360, 88636360, 88813559, 88863633, 88886360, 89027372, 89090906,
///                         89090906, 89138656, 89219992, 89318178, 89331516, 90395480, 90508474, 90909088, 91022724,
///                         91463412, 91463412, 91697933, 91721386, 91932455, 91932455, 92166977, 92790863, 92790863,
///                         92945661, 92945661, 93028788, 93052580, 93183984, 93207816, 93266713, 93266713, 93422306,
///                         93422306, 93504638, 93660628, 93808628, 93925888, 95170116, 95289078, 95328884, 95448045]
/// 3. The XRC returns the median rate and the standard deviation from the BTC/ICP rates.
///     a. The median rate from step 2 is 88813559.
///     b. The standard deviation from step 2 is 3178330.
/// Crypto-fiat pair (retrieve BTC/EUR rate)
/// 0. The XRC retrieves rates from the mock forex sources.
///     a. During collection the rates retrieved are normalized to USD.
///     b. When the collected rates are normalized, then the EUR rate (EUR/USD) is collected (for more information on this collection, see xrc/forex.rs:483).
///         i. For all requests in the following test, this should result in a EUR/USD with the following rates:
///             [917777444, 976400000, 1052938432, 1056100000, 1056900158, 1057200262, 1057421866,
///              1058173944, 1058502845, 1058516154, 1059297297]
/// 1. The XRC retrieves the BTC/USDT rates from the mock exchange responses (request 1 responses).
///     a. BTC/USDT rates: [ 41960000000, 42030000000, 42640000000, 44000000000, 44250000000, 44833000000, 44930000000, 46022000000, 46101000000]
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
///           [38422830017, 38451184548, 38451667997, 38486929114, 38490976280, 38499044560, 38509976251, 38515330948, 38515815203, 38539153477,
///            38555189062, 38563270803, 38574220730, 38603446631, 38654871669, 38719357870, 38834475422, 38863133731, 38863622360, 38899261248,
///            38903351774, 38911506495, 38922555303, 38927967367, 38928456811, 38952045120, 38968252504, 38976420829, 38987488069, 39017027083,
///            39045506957, 39069003067, 39074321000, 39074812283, 39114757593, 39122956627, 39134065476, 39134180145, 39163715544, 39281309055,
///            39463823450, 39492946194, 39493442741, 39533816007, 39542102882, 39553330746, 39583298473, 39611164966, 39640396442, 39640894843,
///            39677246509, 39681418846, 39689736660, 39701006445, 39702151829, 39706526750, 39707025983, 39731086058, 39747617590, 39755949281,
///            39767237866, 39797367660, 39850383164, 39916863784, 40011277739, 40011277739, 40040804482, 40040804482, 40041307917, 40041307917,
///            40078026772, 40078026772, 40082241254, 40082241254, 40090643087, 40090643087, 40102026707, 40102026707, 40107602774, 40107602774,
///            40108107049, 40108107049, 40132410155, 40132410155, 40149108672, 40149108672, 40157524522, 40157524522, 40168927134, 40168927134,
///            40199361269, 40199361269, 40252912282, 40252912282, 40253099956, 40282805154, 40283311632, 40290860837, 40320064424, 40320064424,
///            40320593902, 40321100855, 40324492364, 40332944976, 40344397398, 40362320218, 40370780759, 40374964479, 40382243924, 40412839680,
///            40496194902, 40519786183, 40534183828, 40549688186, 40550198019, 40591651582, 40600160195, 40611688492, 40642458087, 40659696920,
///            40659696920, 40689702172, 40689702172, 40690213765, 40690213765, 40722519509, 40731810464, 40731810464, 40740348456, 40740348456,
///            40751916559, 40751916559, 40752571121, 40753083505, 40764491691, 40782792398, 40782792398, 40794744473, 40803295658, 40814881634,
///            40845805179, 40905247372, 40905247372, 40953897461, 40968449356, 40984119820, 40984635116, 41026532794, 41035132565, 41046784371,
///            41053640089, 41077883618, 41083936055, 41084452605, 41126452325, 41135073040, 41142463123, 41146753224, 41172824637, 41173342305,
///            41177928213, 41201224636, 41215432895, 41224072262, 41235777716, 41267020155, 41301569626, 41390929077, 41493470843, 41524091387,
///            41524613471, 41536969936, 41567063158, 41567622580, 41568145212, 41575776231, 41583245489, 41587581551, 41610639400, 41613932283,
///            41614455497, 41619090535, 41619361608, 41631179304, 41656997027, 41662721320, 41665728952, 41677559814, 41684965139, 41695963084,
///            41709136970, 41726733058, 41727257691, 41744056589, 41754506310, 41765522603, 41769914536, 41772975447, 41778670130, 41787818380,
///            41790533061, 41796343910, 41796869418, 41803802253, 41804327855, 41822195812, 41834373399, 41839597425, 41847063487, 41848367626,
///            41855835253, 41860250348, 41867720095, 41891965920, 41899441327, 41947771713, 41956535284, 41956535284, 41987497551, 41987497551,
///            41988025462, 41988025462, 42017751313, 42025249166, 42030948884, 42030948884, 42039759195, 42039759195, 42051696262, 42051696262,
///            42083556884, 42083556884, 42131559633, 42142409033, 42173508467, 42174038717, 42194924689, 42194924689, 42201845839, 42209917551,
///            42209917551, 42214749442, 42217152297, 42226001639, 42226062877, 42226062877, 42226593788, 42226593788, 42237991588, 42245902261,
///            42246433421, 42269761093, 42269761093, 42269993358, 42278621463, 42278621463, 42289621008, 42290626354, 42290626354, 42298485541,
///            42310496072, 42322668002, 42322668002, 42323340298, 42342552775, 42354573252, 42355105779, 42360507948, 42371684125, 42396913821,
///            42398404459, 42402952755, 42403485890, 42407291794, 42414910436, 42419333221, 42446210966, 42446744644, 42446834028, 42449746627,
///            42449746627, 42451472384, 42455731515, 42467786695, 42469691106, 42490137005, 42499043569, 42499962570, 42511111048, 42543319747,
///            42578937759, 42593904382, 42625337002, 42625872933, 42627573542, 42667019815, 42669448412, 42671060904, 42678392562, 42690510967,
///            42698506391, 42699043241, 42722855590, 42742693521, 42750848781, 42750848781, 42751653025, 42763792232, 42782397220, 42782397220,
///            42782935126, 42782935126, 42796192376, 42814339913, 42826671166, 42826671166, 42835648272, 42835648272, 42843343871, 42843343871,
///            42847811330, 42847811330, 42851135823, 42874960567, 42874960567, 42875499636, 42875499636, 42880275131, 42880275131, 42919330304,
///            42919330304, 42924692812, 42928326833, 42928326833, 42940516205, 42940516205, 42973050245, 42973050245, 42974190865, 43009028035,
///            43009028035, 43045882794, 43102081717, 43102081717, 43408273596, 43408273596, 43445782508, 43477843781, 43478390430, 43480689686,
///            43480689686, 43520360250, 43522837419, 43531960452, 43544321225, 43552476558, 43553024145, 43577312740, 43597547431, 43606686124,
///            43619068115, 43652116263, 43670626751, 43708158579, 43711593568, 43723126209, 43755392150, 43755942289, 43783186707, 43800673013,
///            43809854284, 43822293964, 43855496086, 43884628792, 43884628792, 43917013915, 43917013915, 43917566086, 43917566086, 43959954895,
///            43959959844, 43959959844, 43962462035, 43962462035, 43971553062, 43971677220, 43971677220, 43984162849, 43984162849, 43987177202,
///            43992400559, 43992400559, 43992953678, 43992953678, 44004002332, 44004555597, 44017487612, 44017487612, 44037926693, 44037926693,
///            44047157697, 44047157697, 44049540473, 44058773910, 44059664758, 44059664758, 44071284270, 44093046725, 44093046725, 44104675041,
///            44111744188, 44111744188, 44149655125, 44149655125, 44179900474, 44225441114, 44225441114, 44237104345, 44347570567, 44421553645,
///            44430922636, 44539133510, 44550884484, 44583761278, 44584321832, 44629899390, 44635497705, 44639254480, 44647274103, 44651929665,
///            44680222029, 44680783796, 44685760364, 44726459965, 44735835295, 44748537904, 44782441799, 44819934443, 44822691052, 44897466753,
///            44916906174, 45016306316, 45063498524, 45066263322, 45113702916, 45235990342, 45311455532, 45319541129, 45518685373, 45518685373,
///            45549083567, 45719144915, 45720339981, 45732402599, 45766151306, 45766726727, 45777314266, 45777314266, 45795416129, 45798822161,
///            45810905485, 45813513031, 45823116224, 45836127564, 45844712124, 45845288532, 45870855474, 45892155149, 45901774826, 45914808501,
///            45916632484, 45949596024, 45969080748, 46008587936, 46015977015, 46087564913, 46180954454, 46180954454, 46210167718, 46257996085,
///            46257996085, 46289490721, 46380436848, 46380436848, 46460065280, 46480784859, 46480784859, 46503648831, 46767874109, 46929358864,
///            46929358864, 47001868597, 47134371115, 47215280578, 47268924669, 47384047456, 47435261561, 47486566863, 47610475869, 47610475869,
///            47692202599, 47692202599, 47704780092, 47891699427, 47941906012, 47995317184, 48125415655, 48205701144, 48214303205, 48333297308,
///            48426167684, 48426167684, 48437870498, 48640702875, 48701316363, 48701316363, 48724198063, 48849533459, 48905331829, 48955223570,
///            49161818104, 49246207822, 49342963085, 49342963085, 49449720773, 49449720773, 49615127444, 49700295300, 50145054511, 50231132024,
///            50465164177, 50651570207, 50651570207, 50738517190, 50738517190, 50751898064, 51420561489, 51531814238, 52784267858, 52874875766]
/// 6. The XRC then returns the median and the standard deviation.
///     a. The median rate from step 5 is 42397659140.
///     b. The standard deviation from step 5 is 2767790871.
/// Fiat-crypto pair (retrieve EUR/BTC rate)
/// 0. The instructions are similar to the crypto-fiat pair. The only difference is that the rates are inverted before
///    being returned.
///     a. When inverted, the median rate is 23312466.
///     b. When inverted, the standard deviation is 1445944.
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
            xrc::Exchange::Digifinex(_) => Some("4.00"),
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
        assert_eq!(exchange_rate.metadata.base_asset_num_queried_sources, 9);
        assert_eq!(exchange_rate.metadata.base_asset_num_received_rates, 9);
        assert_eq!(exchange_rate.metadata.quote_asset_num_queried_sources, 9);
        assert_eq!(exchange_rate.metadata.quote_asset_num_received_rates, 9);
        assert_eq!(exchange_rate.metadata.standard_deviation, 3_178_330);
        assert_eq!(exchange_rate.rate, 88_813_559);

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
        assert_eq!(exchange_rate.metadata.standard_deviation, 2_767_790_871);
        assert_eq!(exchange_rate.rate, 42_397_659_140);

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
        assert_eq!(exchange_rate.metadata.standard_deviation, 1_445_944);
        assert_eq!(exchange_rate.rate, 23_586_207);

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
