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
    let rounded_to_nearest_minute = now / 60 * 60;

    let request = GetExchangeRateRequest {
        base_asset: Asset {
            symbol: "ICP".to_string(),
            class: AssetClass::Cryptocurrency,
        },
        quote_asset: Asset {
            symbol: "CXDR".to_string(),
            class: AssetClass::FiatCurrency,
        },
        timestamp: Some(rounded_to_nearest_minute),
    };

    let responses = mock_responses::exchanges::build_responses(
        request.base_asset.symbol.clone(),
        rounded_to_nearest_minute,
        |exchange| match exchange {
            xrc::Exchange::Binance(_) => Some("41.96000000"),
            xrc::Exchange::Coinbase(_) => Some("44.25"),
            xrc::Exchange::KuCoin(_) => Some("44.833"),
            xrc::Exchange::Okx(_) => Some("42.03"),
            xrc::Exchange::GateIo(_) => Some("42.64"),
            xrc::Exchange::Mexc(_) => Some("46.101"),
            xrc::Exchange::Poloniex(_) => Some("46.022"),
        },
    )
    .chain(mock_responses::exchanges::build_responses(
        request.quote_asset.symbol.clone(),
        rounded_to_nearest_minute,
        |exchange| match exchange {
            xrc::Exchange::Binance(_) => Some("41.96000000"),
            xrc::Exchange::Coinbase(_) => Some("44.25"),
            xrc::Exchange::KuCoin(_) => Some("44.833"),
            xrc::Exchange::Okx(_) => Some("42.03"),
            xrc::Exchange::GateIo(_) => Some("42.64"),
            xrc::Exchange::Mexc(_) => Some("46.101"),
            xrc::Exchange::Poloniex(_) => Some("46.022"),
        },
    ))
    .chain(mock_responses::stablecoin::build_responses(
        rounded_to_nearest_minute,
    ))
    .chain(mock_responses::forex::build_responses(now))
    .collect::<Vec<_>>();

    let container = Container::builder()
        .name("get_icp_xdr_rate")
        .exchange_responses(responses)
        .build();

    let request_ = request.clone();
    let exchange_rate_result = run_scenario(container, |container: &Container| {
        Ok(container
            .call_canister::<_, GetExchangeRateResult>("get_exchange_rate", request_)
            .expect("Failed to call canister for rates"))
    })
    .expect("Scenario failed");

    let exchange_rate =
        exchange_rate_result.expect("Failed to retrieve an exchange rate from the canister.");
    assert_eq!(exchange_rate.base_asset, request.base_asset);
    assert_eq!(exchange_rate.quote_asset, request.quote_asset);
    assert_eq!(exchange_rate.timestamp, rounded_to_nearest_minute);
    assert_eq!(exchange_rate.metadata.base_asset_num_queried_sources, 7);
    assert_eq!(exchange_rate.metadata.base_asset_num_received_rates, 7);
    assert_eq!(exchange_rate.metadata.quote_asset_num_queried_sources, 11);
    assert_eq!(exchange_rate.metadata.quote_asset_num_received_rates, 11);
    assert_eq!(exchange_rate.metadata.standard_deviation, 317_420_214);
    assert_eq!(exchange_rate.rate, 33_099_246_254);
}
