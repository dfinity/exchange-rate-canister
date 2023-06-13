use ic_xrc_types::{Asset, AssetClass, GetExchangeRateRequest, GetExchangeRateResult};

use crate::{
    container::{run_scenario, Container},
    mock_responses,
};

/// This test is used to confirm that the exchange rate canister can receive
/// a request to the `get_exchange_rate` endpoint and successfully return a
/// computed rate for the provided assets.
#[ignore]
#[test]
fn can_successfully_retrieve_rate() {
    let now = time::OffsetDateTime::now_utc().unix_timestamp() as u64;
    let timestamp = 1614596340;
    let request = GetExchangeRateRequest {
        timestamp: Some(timestamp),
        quote_asset: Asset {
            symbol: "BTC".to_string(),
            class: AssetClass::Cryptocurrency,
        },
        base_asset: Asset {
            symbol: "ICP".to_string(),
            class: AssetClass::Cryptocurrency,
        },
    };

    let responses = mock_responses::exchanges::build_responses(
        request.base_asset.symbol.clone(),
        timestamp,
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
        timestamp,
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
    .chain(mock_responses::stablecoin::build_responses(timestamp))
    .chain(mock_responses::forex::build_responses(now))
    .collect::<Vec<_>>();

    let container = Container::builder()
        .name("can_successfully_retrieve_rate")
        .exchange_responses(responses)
        .build();

    let exchange_rate_result = run_scenario(container, |container: &Container| {
        Ok(container
            .call_canister::<_, GetExchangeRateResult>("get_exchange_rate", &request)
            .expect("Failed to call canister for rates"))
    })
    .expect("Scenario failed");

    let exchange_rate =
        exchange_rate_result.expect("Failed to retrieve an exchange rate from the canister.");
    assert_eq!(exchange_rate.base_asset, request.base_asset);
    assert_eq!(exchange_rate.quote_asset, request.quote_asset);
    assert_eq!(exchange_rate.timestamp, timestamp);
    assert_eq!(exchange_rate.metadata.base_asset_num_queried_sources, 7);
    assert_eq!(exchange_rate.metadata.base_asset_num_received_rates, 7);
    assert_eq!(exchange_rate.metadata.quote_asset_num_queried_sources, 7);
    assert_eq!(exchange_rate.metadata.quote_asset_num_received_rates, 7);
    assert_eq!(exchange_rate.metadata.standard_deviation, 53_827_575);
    assert_eq!(exchange_rate.rate, 999_999_980);
}
