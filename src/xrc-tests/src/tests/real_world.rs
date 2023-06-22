use std::{thread, time::Duration};

use ic_xrc_types::{Asset, ExchangeRate, GetExchangeRateRequest, GetExchangeRateResult};
use reqwest::Client;
use xrc::{usdt_asset, ForexRateStore, ForexRatesCollector, EXCHANGES, FOREX_SOURCES};

use crate::{
    container::{run_scenario, Container},
    tests::eur_asset,
    ONE_DAY_SECONDS,
};

use super::{btc_asset, icp_asset};

const MAX_DIFFERENCE_PERCENTAGE: f64 = 0.01;

async fn get_crypto_usdt_rate(
    client: &Client,
    asset: Asset,
    timestamp_seconds: u64,
) -> xrc::QueriedExchangeRate {
    let requests = EXCHANGES
        .iter()
        .map(|exchange| {
            let url = exchange.get_url(&asset.symbol, &usdt_asset().symbol, timestamp_seconds);
            let request = client.get(url).build().expect("failed to build request");
            client.execute(request)
        })
        .collect::<Vec<_>>();

    let awaited_results = futures::future::join_all(requests).await;
    let mut rates = vec![];
    for (exchange, request_result) in EXCHANGES.iter().zip(awaited_results.into_iter()) {
        if let Err(error) = request_result {
            eprintln!("{}: {}", exchange, error);
            continue;
        }

        let response = request_result.unwrap();
        let bytes_result = response.bytes().await;
        if let Ok(bytes) = bytes_result {
            match exchange.extract_rate(&bytes) {
                Ok(rate) => rates.push(rate),
                Err(err) => eprintln!("{}: {}", exchange, err),
            }
        }
    }

    xrc::QueriedExchangeRate::new(
        asset,
        usdt_asset(),
        timestamp_seconds,
        &rates,
        EXCHANGES.len(),
        rates.len(),
        None,
    )
}

async fn create_forex_store(client: &Client, timestamp_seconds: u64) -> ForexRateStore {
    let requests = FOREX_SOURCES
        .iter()
        .map(|forex| {
            let url = forex.get_url(timestamp_seconds);
            let request = client.get(url).build().expect("failed to build request");
            client.execute(request)
        })
        .collect::<Vec<_>>();

    let awaited_results = futures::future::join_all(requests).await;
    let mut collector = ForexRatesCollector::new();
    for (forex, request_result) in FOREX_SOURCES.iter().zip(awaited_results.into_iter()) {
        if let Err(error) = request_result {
            eprintln!("{}: {}", forex, error);
            continue;
        }

        let response = request_result.unwrap();
        let bytes_result = response.bytes().await;
        if let Ok(bytes) = bytes_result {
            match forex.extract_rate(&bytes, timestamp_seconds) {
                Ok(rates) => {
                    collector.update(forex.to_string(), timestamp_seconds, rates);
                }
                Err(err) => eprintln!("{}: {}", forex, err),
            }
        }
    }

    let rates = collector
        .get_rates_map(timestamp_seconds)
        .expect("Failed to make rates map");
    let mut store = ForexRateStore::new();
    store.put(timestamp_seconds, rates);
    store
}

#[ignore]
#[test]
fn real_world() {
    let now_seconds = time::OffsetDateTime::now_utc().unix_timestamp() as u64;
    let yesterday_timestamp_seconds = now_seconds
        .saturating_sub(ONE_DAY_SECONDS)
        .saturating_div(ONE_DAY_SECONDS)
        .saturating_mul(ONE_DAY_SECONDS);
    let timestamp_seconds = now_seconds / 60 * 60;

    // Gather up rate information to compare against the canister.
    let client = reqwest::Client::builder()
        .use_rustls_tls()
        .build()
        .expect("Failed to build reqwest client");
    let runtime = tokio::runtime::Runtime::new().expect("Failed to start tokio runtime.");
    let (icp_usdt_rate, btc_usdt_rate, forex_store) = runtime.block_on(async {
        let icp_usdt_rate = get_crypto_usdt_rate(&client, icp_asset(), timestamp_seconds).await;
        let btc_usdt_rate = get_crypto_usdt_rate(&client, btc_asset(), timestamp_seconds).await;
        let forex_store = create_forex_store(&client, yesterday_timestamp_seconds).await;
        (icp_usdt_rate, btc_usdt_rate, forex_store)
    });
    let eur_rate = forex_store
        .get(
            yesterday_timestamp_seconds,
            timestamp_seconds,
            &eur_asset().symbol,
            "USD",
        )
        .expect("EUR/USD not found");

    let container = Container::builder().name("real_world").build();
    run_scenario(container, |container| {
        // Crypto pairs
        let crypto_pair_request = GetExchangeRateRequest {
            timestamp: Some(timestamp_seconds),
            base_asset: icp_asset(),
            quote_asset: btc_asset(),
        };

        let crypto_pair_result = container
            .call_canister::<_, GetExchangeRateResult>("get_exchange_rate", &crypto_pair_request)
            .expect("Failed to call canister for rates");
        let icp_btc_rate = icp_usdt_rate.clone() / btc_usdt_rate.clone();
        let comparison_rate = ExchangeRate::from(icp_btc_rate);
        let crypto_exchange_rate = crypto_pair_result.expect("Failed to retrieve crypto pair rate");
        let diff = crypto_exchange_rate.rate.abs_diff(comparison_rate.rate) as f64;
        let percentage = (diff / comparison_rate.rate as f64) * 100.0;
        assert!(percentage < MAX_DIFFERENCE_PERCENTAGE);

        // Crypto Fiat Pair
        let crypto_fiat_pair_request = GetExchangeRateRequest {
            timestamp: Some(timestamp_seconds),
            base_asset: btc_asset(),
            quote_asset: eur_asset(),
        };

        let crypto_fiat_pair_result = container
            .call_canister::<_, GetExchangeRateResult>(
                "get_exchange_rate",
                &crypto_fiat_pair_request,
            )
            .expect("Failed to call canister for rates");
        let crypto_fiat_exchange_rate =
            crypto_fiat_pair_result.expect("Failed to retrieve crypto/fiat pair rate");
        let btc_eur_rate = btc_usdt_rate / eur_rate;
        println!("{:#?}", ExchangeRate::from(btc_eur_rate));

        println!("{:#?}", crypto_fiat_exchange_rate);

        Ok(())
    })
    .expect("Scenario failed");
}
