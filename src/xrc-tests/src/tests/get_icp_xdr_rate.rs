use ic_xrc_types::{Asset, AssetClass, GetExchangeRateRequest, GetExchangeRateResult};

use crate::tests::{NUM_EXCHANGES, NUM_FOREX_SOURCES};
use crate::{
    container::{run_scenario, Container},
    mock_responses,
};

#[ignore]
#[test]
/// Setup:
/// * Deploy mock FOREX data providers and exchanges
/// * Start replicas and deploy the XRC, configured to use the mock data sources
///
/// Runbook: Assert query for ICP/XDR exchange rate returns the expected value
///
/// Success criteria: All queries return the expected values
///
/// How are the expected values determined (using request 1 as an example):
///
/// 0. The XRC retrieves rates from the mock forex sources.
///     a. During collection the rates retrieved are normalized to USD.
///     b. When the collected rates are normalized, then the computed XDR rate (CXDR/USD) is calculated (for more information on this calculation, see xrc/forex.rs:483).
///         i. For all requests in the following test, this should result in a CXDR/USD with the following rates: [ 1336769190, 1336769190 ].
/// 1. The XRC retrieves the ICP/USDT rates from the mock exchange responses (request 1 responses).
///     i. For request 1, this should result in the following rates discovered:
///          GateIo        Okx         Crypto     Mexc        Coinbase    KuCoin      Bitget      Poloniex
///          [ 3900000000, 3900000000, 3910000000, 3911000000, 3920000000, 3920000000, 3930000000, 4005000000]
/// 2. The XRC retrieves the stablecoin rates from the mock exchanges.
///     i.  For request 1, DAI:  [ 950000000, 990000000, 990000000, 1000000000, 1020000000, 1030927835 ]
///     ii. For request 1, USDC: [ 950000000, 990000000, 990000000, 1000000000, 1020000000, 1030927835 ]
/// 3. The XRC determines if USDT has not depegged. If it has not depegged, it returns the USDT/USD rate.
///     i. For request 1, USDT/USD: [ 970000000, 980392156, 1000000000, 1010101010, 1010101010, 1052631578 ]
/// 4. The XRC then multiplies the USDT/USD rate (step 3) with the ICP/USDT rate (step 1) to get the ICP/USD rate.
///     i. For request 1, this results in the following rates:
///        [ 3783000000, 3783000000, 3792700000, 3793670000, 3802400000, 3802400000, 3812100000, 3823529408, 3823529408,
///          3833333329, 3834313722, 3843137251, 3843137251, 3852941173, 3884850000, 3900000000, 3900000000, 3910000000,
///          3911000000, 3920000000, 3920000000, 3926470584, 3930000000, 3939393939, 3939393939, 3939393939, 3939393939,
///          3949494949, 3949494949, 3950505050, 3950505050, 3959595959, 3959595959, 3959595959, 3959595959, 3969300000
//           3969300000, 4005000000, 4045454545, 4045454545, 4105263154, 4105263154, 4115789469, 4116842101, 4126315785,
//           4126315785, 4126500000, 4215789469, ]
/// 5. The XRC divides the ICP/USD by the forex rate CXDR/USD. The division works by inverting CXDR/USD to USD/CXDR then multiplying
///    USD/CXDR and ICP/USD resulting in ICP/CXDR.
///     i. For request 1, this results in the following rates:
///        [ 2860468811, 2860468811, 2860468811, 2860468811, 2867803346, 2867803346, 2868536800, 2868536800, 2875137882, 2875137882,
///          2875137882, 2875137882, 2882472418, 2882472418, 2888501408, 2888501408, 2888501408, 2888501408, 2888501408, 2888501408,
///          2888501408, 2888501408, 2895907822, 2895907822, 2895907822, 2895907822, 2896648464, 2896648464, 2896648464, 2896648464, 2903314236, 2903314236, 2903314236, 2903314236, 2903314236, 2903314236, 2903314236, 2903314236, 2910720650, 2910720650, 2910720650, 2910720650, 2937481433, 2937481433, 2946854972, 2946854972, 2946854972, 2946854972, 2954411010, 2954411010, 2955166614, 2955166614, 2961967049, 2961967049, 2961967049, 2961967049, 2966268754, 2966268754, 2966268754, 2966268754, 2969523087, 2969523087, 3007915659, 3007915659, 3007915659, 3007915659, 3007915659, 3007915659, 3007915659, 3007915659, 3015628263, 3015628263, 3015628263, 3015628263, 3016399524, 3016399524, 3016399524, 3016399524, 3023340868, 3023340868, 3023340868, 3023340868, 3023340868, 3023340868, 3023340868, 3023340868, 3026193375, 3026193375, 3031053472, 3031053472, 3031053472, 3031053472, 3071240197, 3071240197, 3071240197, 3071240197, 3079115172, 3079115172, 3079902669, 3079902669, 3086990147, 3086990147, 3086990147, 3086990147, 3088898004, 3088898004, 3088898004, 3088898004, 3094865122, 3094865122, 3153927433, 3153927433 ]
/// 6. The XRC returns the median rate and the standard deviation from the ICP/CXDR rates.
///    i. For request 1, the median rate is  2964117901.
///    ii. For request 1, the std dev is  77329936.
fn get_icp_xdr_rate() {
    let now_seconds = time::OffsetDateTime::now_utc().unix_timestamp() as u64;
    let request_1_timestamp_seconds = now_seconds / 60 * 60;
    let request_2_timestamp_seconds = request_1_timestamp_seconds - 60;
    let request_3_timestamp_seconds = request_2_timestamp_seconds - 60;

    // Create the mock responses.
    // Request 1 mock exchange responses.
    let responses = mock_responses::exchanges::build_responses(
        "ICP".to_string(),
        request_1_timestamp_seconds,
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
    // Request 2 mock exchange responses.
    .chain(mock_responses::exchanges::build_responses(
        "ICP".to_string(),
        request_2_timestamp_seconds,
        |exchange| match exchange {
            xrc::Exchange::Coinbase(_) => Some("4.30"),
            xrc::Exchange::KuCoin(_) => Some("4.30"),
            xrc::Exchange::Okx(_) => Some("4.28"),
            xrc::Exchange::GateIo(_) => Some("4.28"),
            xrc::Exchange::Mexc(_) => Some("4.291"),
            xrc::Exchange::Poloniex(_) => Some("4.38"),
            xrc::Exchange::CryptoCom(_) => Some("4.29"),
            xrc::Exchange::Bitget(_) => Some("4.30"),
        },
    ))
    // Request 3 mock exchange responses.
    .chain(mock_responses::exchanges::build_responses(
        "ICP".to_string(),
        request_3_timestamp_seconds,
        |exchange| match exchange {
            xrc::Exchange::Coinbase(_) => Some("5.18"),
            xrc::Exchange::KuCoin(_) => Some("5.18"),
            xrc::Exchange::Okx(_) => Some("5.16"),
            xrc::Exchange::GateIo(_) => Some("5.16"),
            xrc::Exchange::Mexc(_) => Some("5.171"),
            xrc::Exchange::Poloniex(_) => Some("5.26"),
            xrc::Exchange::CryptoCom(_) => Some("5.17"),
            xrc::Exchange::Bitget(_) => Some("5.18"),
        },
    ))
    .chain(mock_responses::stablecoin::build_responses(
        request_1_timestamp_seconds,
    ))
    .chain(mock_responses::stablecoin::build_responses(
        request_2_timestamp_seconds,
    ))
    .chain(mock_responses::stablecoin::build_responses(
        request_3_timestamp_seconds,
    ))
    .chain(mock_responses::forex::build_common_responses(now_seconds))
    .collect::<Vec<_>>();

    let container = Container::builder()
        .name("get_icp_xdr_rate")
        .exchange_responses(responses)
        .build();

    run_scenario(container, |container: &Container| {
        let request = GetExchangeRateRequest {
            base_asset: Asset {
                symbol: "ICP".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            quote_asset: Asset {
                symbol: "CXDR".to_string(),
                class: AssetClass::FiatCurrency,
            },
            timestamp: Some(request_1_timestamp_seconds),
        };

        let exchange_rate_result = container
            .call_canister::<_, GetExchangeRateResult>("get_exchange_rate", &request)
            .expect("Failed to call canister for rates");
        let exchange_rate =
            exchange_rate_result.expect("Failed to retrieve an exchange rate from the canister.");
        assert_eq!(exchange_rate.base_asset, request.base_asset);
        assert_eq!(exchange_rate.quote_asset, request.quote_asset);
        assert_eq!(exchange_rate.timestamp, request_1_timestamp_seconds);
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
        assert_eq!(exchange_rate.metadata.standard_deviation, 77_329_936);
        assert_eq!(exchange_rate.rate, 2_964_117_901);

        let request = GetExchangeRateRequest {
            base_asset: Asset {
                symbol: "ICP".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            quote_asset: Asset {
                symbol: "CXDR".to_string(),
                class: AssetClass::FiatCurrency,
            },
            timestamp: Some(request_2_timestamp_seconds),
        };
        let exchange_rate_result = container
            .call_canister::<_, GetExchangeRateResult>("get_exchange_rate", &request)
            .expect("Failed to call canister for rates");
        let exchange_rate =
            exchange_rate_result.expect("Failed to retrieve an exchange rate from the canister.");
        assert_eq!(exchange_rate.base_asset, request.base_asset);
        assert_eq!(exchange_rate.quote_asset, request.quote_asset);
        assert_eq!(exchange_rate.timestamp, request_2_timestamp_seconds);
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
        assert_eq!(exchange_rate.metadata.standard_deviation, 83_722_994);
        assert_eq!(exchange_rate.rate, 3_246_552_891);

        let request = GetExchangeRateRequest {
            base_asset: Asset {
                symbol: "ICP".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            quote_asset: Asset {
                symbol: "CXDR".to_string(),
                class: AssetClass::FiatCurrency,
            },
            timestamp: Some(request_3_timestamp_seconds),
        };
        let exchange_rate_result = container
            .call_canister::<_, GetExchangeRateResult>("get_exchange_rate", &request)
            .expect("Failed to call canister for rates");
        let exchange_rate =
            exchange_rate_result.expect("Failed to retrieve an exchange rate from the canister.");
        assert_eq!(exchange_rate.base_asset, request.base_asset);
        assert_eq!(exchange_rate.quote_asset, request.quote_asset);
        assert_eq!(exchange_rate.timestamp, request_3_timestamp_seconds);
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
        assert_eq!(exchange_rate.metadata.standard_deviation, 99_654_513);
        assert_eq!(exchange_rate.rate, 3_910_627_669);

        Ok(())
    })
    .expect("Scenario failed");
}
