use ic_xrc_types::{Asset, AssetClass, GetExchangeRateRequest, GetExchangeRateResult};

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
fn get_icp_xdr_rate() {
    let now = time::OffsetDateTime::now_utc().unix_timestamp() as u64;
    let set1_timestamp = now / 60 * 60;
    let set2_timestamp = set1_timestamp - 60;
    let set3_timestamp = set2_timestamp - 60;

    // Create the mock responses.
    let responses =
        mock_responses::exchanges::build_responses("ICP".to_string(), set1_timestamp, |exchange| {
            match exchange {
                xrc::Exchange::Binance(_) => Some("3.91"),
                xrc::Exchange::Coinbase(_) => Some("3.92"),
                xrc::Exchange::KuCoin(_) => Some("3.92"),
                xrc::Exchange::Okx(_) => Some("3.90"),
                xrc::Exchange::GateIo(_) => Some("3.90"),
                xrc::Exchange::Mexc(_) => Some("3.911"),
                xrc::Exchange::Poloniex(_) => Some("4.005"),
            }
        })
        .chain(mock_responses::exchanges::build_responses(
            "ICP".to_string(),
            set2_timestamp,
            |exchange| match exchange {
                xrc::Exchange::Binance(_) => Some("4.29"),
                xrc::Exchange::Coinbase(_) => Some("4.30"),
                xrc::Exchange::KuCoin(_) => Some("4.30"),
                xrc::Exchange::Okx(_) => Some("4.28"),
                xrc::Exchange::GateIo(_) => Some("4.28"),
                xrc::Exchange::Mexc(_) => Some("4.291"),
                xrc::Exchange::Poloniex(_) => Some("4.38"),
            },
        ))
        .chain(mock_responses::exchanges::build_responses(
            "ICP".to_string(),
            set3_timestamp,
            |exchange| match exchange {
                xrc::Exchange::Binance(_) => Some("5.17"),
                xrc::Exchange::Coinbase(_) => Some("5.18"),
                xrc::Exchange::KuCoin(_) => Some("5.18"),
                xrc::Exchange::Okx(_) => Some("5.16"),
                xrc::Exchange::GateIo(_) => Some("5.16"),
                xrc::Exchange::Mexc(_) => Some("5.171"),
                xrc::Exchange::Poloniex(_) => Some("5.26"),
            },
        ))
        .chain(mock_responses::stablecoin::build_responses(set1_timestamp))
        .chain(mock_responses::stablecoin::build_responses(set2_timestamp))
        .chain(mock_responses::stablecoin::build_responses(set3_timestamp))
        .chain(mock_responses::forex::build_responses(now))
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
            timestamp: Some(set1_timestamp),
        };

        let exchange_rate_result = container
            .call_canister::<_, GetExchangeRateResult>("get_exchange_rate", &request)
            .expect("Failed to call canister for rates");
        let exchange_rate =
            exchange_rate_result.expect("Failed to retrieve an exchange rate from the canister.");
        assert_eq!(exchange_rate.base_asset, request.base_asset);
        assert_eq!(exchange_rate.quote_asset, request.quote_asset);
        assert_eq!(exchange_rate.timestamp, set1_timestamp);
        assert_eq!(exchange_rate.metadata.base_asset_num_queried_sources, 7);
        assert_eq!(exchange_rate.metadata.base_asset_num_received_rates, 7);
        assert_eq!(exchange_rate.metadata.quote_asset_num_queried_sources, 11);
        assert_eq!(exchange_rate.metadata.quote_asset_num_received_rates, 11);
        assert_eq!(exchange_rate.metadata.standard_deviation, 81_973_860);
        assert_eq!(exchange_rate.rate, 2_946_951_476);

        let request = GetExchangeRateRequest {
            base_asset: Asset {
                symbol: "ICP".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            quote_asset: Asset {
                symbol: "CXDR".to_string(),
                class: AssetClass::FiatCurrency,
            },
            timestamp: Some(set2_timestamp),
        };
        let exchange_rate_result = container
            .call_canister::<_, GetExchangeRateResult>("get_exchange_rate", &request)
            .expect("Failed to call canister for rates");
        let exchange_rate =
            exchange_rate_result.expect("Failed to retrieve an exchange rate from the canister.");
        assert_eq!(exchange_rate.base_asset, request.base_asset);
        assert_eq!(exchange_rate.quote_asset, request.quote_asset);
        assert_eq!(exchange_rate.timestamp, set2_timestamp);
        assert_eq!(exchange_rate.metadata.base_asset_num_queried_sources, 7);
        assert_eq!(exchange_rate.metadata.base_asset_num_received_rates, 7);
        assert_eq!(exchange_rate.metadata.quote_asset_num_queried_sources, 11);
        assert_eq!(exchange_rate.metadata.quote_asset_num_received_rates, 11);
        assert_eq!(exchange_rate.metadata.standard_deviation, 88_785_978);
        assert_eq!(exchange_rate.rate, 3_234_090_337);

        let request = GetExchangeRateRequest {
            base_asset: Asset {
                symbol: "ICP".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            quote_asset: Asset {
                symbol: "CXDR".to_string(),
                class: AssetClass::FiatCurrency,
            },
            timestamp: Some(set3_timestamp),
        };
        let exchange_rate_result = container
            .call_canister::<_, GetExchangeRateResult>("get_exchange_rate", &request)
            .expect("Failed to call canister for rates");
        let exchange_rate =
            exchange_rate_result.expect("Failed to retrieve an exchange rate from the canister.");
        assert_eq!(exchange_rate.base_asset, request.base_asset);
        assert_eq!(exchange_rate.quote_asset, request.quote_asset);
        assert_eq!(exchange_rate.timestamp, set3_timestamp);
        assert_eq!(exchange_rate.metadata.base_asset_num_queried_sources, 7);
        assert_eq!(exchange_rate.metadata.base_asset_num_received_rates, 7);
        assert_eq!(exchange_rate.metadata.quote_asset_num_queried_sources, 11);
        assert_eq!(exchange_rate.metadata.quote_asset_num_received_rates, 11);
        assert_eq!(exchange_rate.metadata.standard_deviation, 105_677_402);
        assert_eq!(exchange_rate.rate, 3_899_043_491);

        Ok(())
    })
    .expect("Scenario failed");
}
