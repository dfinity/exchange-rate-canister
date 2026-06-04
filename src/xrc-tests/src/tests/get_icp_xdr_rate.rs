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
///     1. During collection the rates retrieved are normalized to USD.
///     2. When the collected rates are normalized, then the computed XDR rate (CXDR/USD) is calculated (for more information on this calculation, see xrc/forex.rs:483).
///         1. For all requests in the following test, this should result in a CXDR/USD with the following rates: [ 1336769190, 1336769190 ].
/// 1. The XRC retrieves the ICP/USDT rates from the mock exchange responses (request 1 responses).
///     1. For request 1, this should result in the following rates discovered:
///        GateIo        Okx         Crypto     Mexc        Coinbase    KuCoin      Bitget      Digifinex   Poloniex
///        [ 3900000000, 3900000000, 3910000000, 3911000000, 3920000000, 3920000000, 3930000000, 4000000000, 4005000000]
/// 2. The XRC retrieves the stablecoin rates (USDS and USDC, each quoted in USDT) from the mock exchanges.
/// 3. The XRC determines if USDT has not depegged. If it has not depegged, it returns the USDT/USD rate.
/// 4. The XRC then multiplies the USDT/USD rate (step 3) with the ICP/USDT rate (step 1) to get the ICP/USD rate.
/// 5. The XRC divides the ICP/USD by the forex rate CXDR/USD. The division works by inverting CXDR/USD to USD/CXDR then multiplying
///    USD/CXDR and ICP/USD resulting in ICP/CXDR.
/// 6. The XRC returns the median rate and the standard deviation of the resulting ICP/CXDR rates.
///    The concrete expected median rate and standard deviation for each request are asserted below.
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
            xrc::Exchange::Digifinex(_) => Some("4.00"),
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
            xrc::Exchange::Digifinex(_) => Some("4.20"),
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
            xrc::Exchange::Digifinex(_) => Some("5.20"),
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
        assert_eq!(exchange_rate.metadata.standard_deviation, 81_140_854);
        assert_eq!(exchange_rate.rate, 3_015_694_093);

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
        assert_eq!(exchange_rate.metadata.standard_deviation, 88_887_175);
        assert_eq!(exchange_rate.rate, 3_301_066_679);

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
        assert_eq!(exchange_rate.metadata.standard_deviation, 102_164_274);
        assert_eq!(exchange_rate.rate, 3_987_503_442);

        Ok(())
    })
    .expect("Scenario failed");
}
