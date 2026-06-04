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
///     1. ICP/USDT rates:
///           GateIo      Okx         Crypto      Mexc        Coinbase    KuCoin      Bitget      Digifinex   Poloniex
///          [ 3900000000, 3900000000, 3910000000, 3911000000, 3920000000, 3920000000, 3930000000, 4000000000, 4005000000, ]
/// 1. The XRC retrieves the BTC/USDT rate.
///     1. BTC/USDT rates: [ 41960000000, 42030000000, 42640000000, 44250000000, 44250000000, 44833000000, 44930000000, 46022000000, 46101000000, ]
/// 2. The XRC divides ICP/USDT by BTC/USDT. The division inverts BTC/USDT to USDT/BTC then multiplies ICP/USDT and USDT/BTC
///    to get the resulting ICP/BTC rate.
///     1. ICP/BTC rates:
///        [84596861, 84596861, 84742078, 84742078, 84813776, 84835468, 84959365, 84981094, 85030691,
///        85030691, 85176652, 85176652, 85247606, 85393940, 86766012, 86801687, 86801687, 86874469,
///        86914952, 86989492, 86989492, 87023595, 87024256, 87046512, 87212542, 87234847, 87246824,
///        87246824, 87435592, 87435592, 87469392, 87658642, 88135593, 88135593, 88361581, 88384180,
///        88587570, 88587570, 88636360, 88636360, 88813559, 88863633, 88886360, 89027372, 89090906,
///        89090906, 89138656, 89219992, 89318178, 89331516, 90395480, 90508474, 90909088, 91022724,
///        91463412, 91463412, 91697933, 91721386, 91932455, 91932455, 92166977, 92790863, 92790863,
///        92945661, 92945661, 93028788, 93052580, 93183984, 93207816, 93266713, 93266713, 93422306,
///        93422306, 93504638, 93660628, 93808628, 93925888, 95170116, 95289078, 95328884, 95448045]
/// 3. The XRC returns the median rate and the standard deviation from the BTC/ICP rates.
///     1. The median rate from step 2 is 88813559.
///     2. The standard deviation from step 2 is 3178330.
///
/// Crypto-fiat pair (retrieve BTC/EUR rate)
/// 0. The XRC retrieves rates from the mock forex sources.
///     1. During collection the rates retrieved are normalized to USD.
///     2. When the collected rates are normalized, then the EUR rate (EUR/USD) is collected (for more information on this collection, see xrc/forex.rs:483).
///         1. For all requests in the following test, this should result in a EUR/USD with the following rates:
///             [917777444, 976400000, 1052938432, 1056100000, 1056900158, 1057200262, 1057421866,
///              1058173944, 1058502845, 1058516154, 1059297297]
/// 1. The XRC retrieves the BTC/USDT rates from the mock exchange responses (request 1 responses).
///     1. BTC/USDT rates: [ 41960000000, 42030000000, 42640000000, 44000000000, 44250000000, 44833000000, 44930000000, 46022000000, 46101000000]
/// 2. The XRC retrieves the stablecoin rates (USDS and USDC, each quoted in USDT) from the mock exchanges.
/// 3. The XRC determines the USDT/USD rate.
/// 4. The XRC then multiplies the USDT/USD rate (step 3) with the BTC/USDT rate (step 1) to get the BTC/USD rate.
/// 5. The XRC divides the BTC/USD by the forex rate EUR/USD. The division works by inverting EUR/USD to USD/EUR then multiplying
///    USD/EUR and BTC/USD resulting in BTC/EUR.
/// 6. The XRC then returns the median rate and the standard deviation of the resulting BTC/EUR rates.
///    The concrete expected median rate and standard deviation are asserted below.
///
/// Fiat-crypto pair (retrieve EUR/BTC rate)
/// 0. The instructions are similar to the crypto-fiat pair. The only difference is that the rates are inverted before
///    being returned. The concrete expected median rate and standard deviation are asserted below.
///
/// Fiat pair (retrieve EUR/JPY rate)
/// 0. The XRC retrieves rates from the mock forex sources.
///     1. During collection the rates retrieved are normalized to USD.
///     2. When the collected rates are normalized, then the EUR rate (EUR/USD) is collected (for more information on this collection, see xrc/forex.rs:483).
///         1. For all requests in the following test, this should result in a EUR/USD with the following rates:
///             [917777444, 976400000, 1052938432, 1056100000, 1056900158, 1057200262, 1057421866,
///              1058173944, 1058502845, 1058516154, 1059297297]
///         2. For all requests in the following test, this should result in a JPY/USD with the following rates:
///             [6900840, 7346082, 7350873, 7369729, 7380104, 7390111, 7395293, 7395822, 7395930, 7399602]
/// 1. The XRC divides EUR/USD by JPY/USD. The division inverts JPY/USD to USD/JPY then multiplies EUR/USD and USD/JPY
///    to get the resulting EUR/JPY rate.
///     1. EUR/JPY rates should then include:
///        [124030649755, 124092229644, 124094041743, 124102918437, 124358334787, 124533404687, 124852849994, 124934277074, 131953042878, 132018556151,
///        132020483996, 132029927684, 132301658621, 132487911020, 132827760729, 132914388921, 132995033068, 135589467313, 141490021504, 142296630547, 
///        142367279300, 142369358266, 142379542230, 142672573719, 142723892446, 142794753330, 142796838538, 142807053080, 142832027721, 142872584497, 
///        142873426145, 142902532594, 142902942293, 142905029081, 142915251362, 142943519205, 142945606586, 142955831769, 142973482171, 142975569989, 
///        142985797316, 143048618694, 143050417305, 143100964430, 143119640802, 143121440305, 143121730754, 143123530284, 143131968536, 143133768195, 
///        143155982847, 143209385395, 143227058260, 143229149781, 143239395247, 143239916129, 143250049321, 143280076540, 143302419939, 143333334966, 
///        143410993537, 143426548595, 143428351957, 143451714709, 143481784200, 143534196401, 143628462457, 143630268357, 143670010350, 143736261807, 
///        143763709688, 143778862455, 143819688082, 143849834706, 143872632785, 143913485038, 143943651322, 143996889212, 143998699745, 144090801735, 
///        144092613449, 144104965083, 144198948091, 144250173885, 146264455573, 146337074309, 146339211245, 146349679180, 146650881613, 146857334644, 
///        147234043901, 147330067646, 152581197651, 153039340138, 153155290949, 153198778988, 153230891601, 153387536154, 153389464760, 153502660110, 
///        155557713956, 156024793773, 156143006525, 156187342918, 156220081975, 156379782312, 156381748540, 156497152077, 156835799409, 159895313434]
/// 2. The XRC then return the median and the standard deviation.
///     1. The median rate from the group of rates in step 1.a.: 143239655688.
///     2. The standard deviation of the group of rates in step 1.a.: 7823121871.
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
        assert_eq!(exchange_rate.metadata.standard_deviation, 2_745_555_944);
        assert_eq!(exchange_rate.rate, 43_072_574_134);

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
        assert_eq!(exchange_rate.metadata.standard_deviation, 1_401_006);
        assert_eq!(exchange_rate.rate, 23_216_630);

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
        assert_eq!(exchange_rate.metadata.standard_deviation, 7_823_121_871);
        assert_eq!(exchange_rate.rate, 143_239_655_688);

        Ok(())
    })
    .expect("Scenario failed");
}
