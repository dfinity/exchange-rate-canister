use crate::{
    container::{run_scenario, Container},
    mock_responses, ONE_DAY_SECONDS,
};

/// Setup:
/// * Deploy mock FOREX data providers and exchanges, some of which are configured to be malicious
/// * Start replicas and deploy the XRC, configured to use the mock data sources
///
/// Runbook:
/// * Request exchange rate for various cryptocurrency and fiat currency pairs
/// * Assert that the returned rates correspond to the expected values and that the confidence is lower due to the erroneous responses
///
/// Success criteria:
/// * All queries return the expected values
///
#[ignore]
#[test]
fn misbehavior() {
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
            xrc::Exchange::Binance(_) => Some("3.91"),
            xrc::Exchange::Coinbase(_) => Some("3.92"),
            xrc::Exchange::KuCoin(_) => Some("3.92"),
            xrc::Exchange::Okx(_) => Some("3.90"),
            xrc::Exchange::GateIo(_) => Some("3.90"),
            xrc::Exchange::Mexc(_) => Some("3.911"),
            xrc::Exchange::Poloniex(_) => Some("4.005"),
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
        .name("misbehavior")
        .exchange_responses(responses)
        .build();

    run_scenario(container, |container| {
        //
        Ok(())
    })
    .expect("Scenario failed");
}
