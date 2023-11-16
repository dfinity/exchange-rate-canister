use std::time::Instant;

use ic_xrc_types::{
    Asset, AssetClass, ExchangeRate, ExchangeRateMetadata, GetExchangeRateRequest,
    GetExchangeRateResult,
};

use crate::{
    container::{run_scenario, Container},
    mock_responses, ONE_MINUTE_SECONDS,
};

/// Setup:
/// * Deploy mock FOREX data providers and exchanges with a large response delay
///
/// Start replicas and deploy the XRC, configured to use the mock data sources
///
/// Runbook:
///
/// 1. Request the same exchange rate many times per second for a bounded time, e.g., 1 minute
/// 2. Assert that all requests are answered with the same rate with a small delay after the first response (due to caching in the XRC)
///
/// Success criteria:
///
/// * All queries are handled correctly
///
/// The expected values are determined as follows:
///
/// Crypto-pair (retrieve ICP/BTC rate)
/// 0. The XRC retrieves the ICP/USDT rate.
///     a. ICP/USDT rates: [ 3900000000, 3900000000, 3910000000, 3911000000, 3920000000, 3920000000, 4005000000, ]
/// 1. The XRC retrieves the BTC/USDT rate.
///     a. BTC/USDT rates: [ 41960000000, 42030000000, 42640000000, 44250000000, 44833000000, 46022000000, 46101000000, ]
/// 2. The XRC divides ICP/USDT by BTC/USDT. The division inverts BTC/USDT to USDT/BTC then multiplies ICP/USDT and USDT/BTC
///    to get the resulting ICP/BTC rate.
///     a. ICP/BTC rates: [ 84596861, 84596861, 84742078, 84742078, 84813776, 84835468, 84959365, 84981094,
///                         85030691, 85030691, 85176652, 85176652, 86874469, 86989492, 86989492, 87023595,
///                         87212542, 87234847, 87435592, 87435592, 88135593, 88135593, 88361581, 88384180,
///                         88587570, 88587570, 89331516, 90508474, 91463412, 91463412, 91697933, 91721386,
///                         91932455, 91932455, 92790863, 92790863, 92945661, 92945661, 93028788, 93052580,
///                         93183984, 93207816, 93266713, 93266713, 93422306, 93422306, 93925888, 95289078,
///                         95448045 ]
/// 3. The XRC returns the median rate and the standard deviation from the BTC/ICP rates.
///     a. The median rate from step 2 is 88587570.
///     b. The standard deviation from step 2 is 3483761.
#[ignore]
#[test]
fn caching() {
    struct ScenarioResult {
        result: GetExchangeRateResult,
        time_passed_millis: u128,
    }

    let mut samples: Vec<ScenarioResult> = vec![];
    let now = time::OffsetDateTime::now_utc().unix_timestamp() as u64;
    let timestamp_seconds = 1614596340;
    let request = GetExchangeRateRequest {
        timestamp: Some(timestamp_seconds),
        quote_asset: Asset {
            symbol: "BTC".to_string(),
            class: AssetClass::Cryptocurrency,
        },
        base_asset: Asset {
            symbol: "ICP".to_string(),
            class: AssetClass::Cryptocurrency,
        },
    };

    let expected_exchange_rate = ExchangeRate {
        base_asset: Asset {
            symbol: "ICP".to_string(),
            class: AssetClass::Cryptocurrency,
        },
        quote_asset: Asset {
            symbol: "BTC".to_string(),
            class: AssetClass::Cryptocurrency,
        },
        timestamp: timestamp_seconds,
        rate: 88_587_570,
        metadata: ExchangeRateMetadata {
            decimals: 9,
            base_asset_num_queried_sources: 7,
            base_asset_num_received_rates: 7,
            quote_asset_num_queried_sources: 7,
            quote_asset_num_received_rates: 7,
            standard_deviation: 3_483_761,
            forex_timestamp: None,
        },
    };

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
            xrc::Exchange::Bybit(_) => Some("3.91"),
        },
    )
    .chain(mock_responses::exchanges::build_common_responses(
        request.quote_asset.symbol.clone(),
        timestamp_seconds,
    ))
    .chain(mock_responses::stablecoin::build_responses(
        timestamp_seconds,
    ))
    .chain(mock_responses::forex::build_common_responses(now))
    .collect::<Vec<_>>();
    let container = Container::builder()
        .name("caching")
        .exchange_responses(responses)
        .build();

    run_scenario(container, |container| {
        let run_scenario_instant = Instant::now();
        while run_scenario_instant.elapsed().as_secs() <= ONE_MINUTE_SECONDS {
            let iteration_instant = Instant::now();
            let result = container
                .call_canister::<_, GetExchangeRateResult>("get_exchange_rate", &request)
                .expect("Failed to call canister for rates");

            samples.push(ScenarioResult {
                result,
                time_passed_millis: iteration_instant.elapsed().as_millis(),
            });
        }

        // Check that all samples were successful.
        assert!(samples.len() > 1);
        for sample in &samples {
            match &sample.result {
                Ok(exchange_rate) => {
                    assert_eq!(*exchange_rate, expected_exchange_rate);
                }
                Err(error) => panic!("Received an error from the XRC: {:?}", error),
            };
        }

        // Compare the response times of the samples to ensure the cache mechanism was used
        // for subsequent requests after the first.
        let first_sample_millis = samples[0].time_passed_millis as f64;
        let threshold = first_sample_millis * 0.6;
        println!("threshold = {}", threshold);
        for (n, sample) in samples.iter().skip(1).enumerate() {
            let current_sample_millis = sample.time_passed_millis as f64;
            println!(
                "r1 = {}, r{} = {}",
                first_sample_millis, n, current_sample_millis
            );

            assert!(current_sample_millis / first_sample_millis <= threshold);
        }

        Ok(())
    })
    .expect("Scenario failed");
}
