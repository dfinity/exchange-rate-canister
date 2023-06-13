use ic_xrc_types::{Asset, AssetClass, GetExchangeRateRequest, GetExchangeRateResult};
use xrc::{usdt_asset, EXCHANGES};

use crate::{
    container::{run_scenario, Container, ExchangeResponse},
    mock_responses,
    tests::{build_crypto_exchange_response, get_sample_json_for_exchange},
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

    let responses = EXCHANGES
        .iter()
        .flat_map(|exchange| {
            let json = get_sample_json_for_exchange(exchange);
            [
                build_crypto_exchange_response(
                    exchange,
                    &request.base_asset,
                    rounded_to_nearest_minute,
                    json.clone(),
                ),
                {
                    let asset = &request.quote_asset;
                    ExchangeResponse::builder()
                        .name(exchange.to_string())
                        .url(exchange.get_url(
                            &asset.symbol,
                            &usdt_asset().symbol,
                            rounded_to_nearest_minute,
                        ))
                        .json(json)
                        .build()
                },
            ]
        })
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
