use futures::future::join_all;
use ic_xrc_types::{Asset, AssetClass};

use super::Exchange;
use crate::{
    http::{CanisterHttpRequest, HttpRequestClient},
    usdt_asset, utils, CallExchangeArgs, CallExchangeError, QueriedExchangeRate, EXCHANGES,
    LOG_PREFIX, USDT,
};

pub(crate) async fn call_exchange(
    client: &impl HttpRequestClient,
    exchange: &Exchange,
    args: CallExchangeArgs,
) -> Result<u64, CallExchangeError> {
    let url = exchange.get_url(
        &args.base_asset.symbol,
        &args.quote_asset.symbol,
        args.timestamp,
    );
    let context = exchange
        .encode_context()
        .map_err(|error| CallExchangeError::Candid {
            exchange: exchange.to_string(),
            error: format!("Failure while encoding context: {}", error),
        })?;
    let arg = CanisterHttpRequest::new()
        .get(&url)
        .transform_context("transform_exchange_http_response", context)
        .max_response_bytes(exchange.max_response_bytes())
        .build();

    let response = client
        .call(arg)
        .await
        .map_err(|error| CallExchangeError::Http {
            exchange: exchange.to_string(),
            error,
        })?;

    Exchange::decode_response(&response.body).map_err(|error| CallExchangeError::Candid {
        exchange: exchange.to_string(),
        error: format!("Failure while decoding response: {}", error),
    })
}

pub(crate) async fn get_cryptocurrency_usdt_rate(
    client: &impl HttpRequestClient,
    asset: &Asset,
    timestamp: u64,
) -> Result<QueriedExchangeRate, CallExchangeError> {
    let futures = EXCHANGES.iter().filter_map(|exchange| {
        if !exchange.is_available() {
            return None;
        }

        Some(call_exchange(
            client,
            exchange,
            CallExchangeArgs {
                timestamp,
                quote_asset: usdt_asset(),
                base_asset: asset.clone(),
            },
        ))
    });
    let results = join_all(futures).await;

    let mut rates = vec![];
    let mut errors = vec![];
    for result in results {
        match result {
            Ok(rate) => rates.push(rate),
            Err(err) => {
                ic_cdk::println!(
                    "{} Timestamp: {}, Asset: {:?}, Error: {}",
                    LOG_PREFIX,
                    timestamp,
                    asset,
                    err,
                );
                errors.push(err);
            }
        }
    }

    if rates.is_empty() {
        return Err(CallExchangeError::NoRatesFound);
    }

    rates.sort();

    Ok(QueriedExchangeRate::new(
        asset.clone(),
        Asset {
            symbol: USDT.to_string(),
            class: AssetClass::Cryptocurrency,
        },
        timestamp,
        &rates,
        rates.len() + errors.len(),
        rates.len(),
        None,
    ))
}

pub(crate) async fn get_stablecoin_rates(
    client: &impl HttpRequestClient,
    symbols: &[&str],
    timestamp: u64,
) -> Vec<Result<QueriedExchangeRate, CallExchangeError>> {
    join_all(
        symbols
            .iter()
            .map(|symbol| get_stablecoin_rate(client, symbol, timestamp)),
    )
    .await
}

async fn get_stablecoin_rate(
    client: &impl HttpRequestClient,
    symbol: &str,
    timestamp: u64,
) -> Result<QueriedExchangeRate, CallExchangeError> {
    let futures = EXCHANGES.iter().filter_map(|exchange| {
        if !exchange.is_available() {
            return None;
        }
        let (base_symbol, quote_symbol) = exchange
            .supported_stablecoin_pairs()
            .iter()
            .find(|pair| pair.0 == symbol || pair.1 == symbol)?;
        let invert = *base_symbol == USDT;

        let future = async move {
            let result = call_exchange(
                client,
                exchange,
                CallExchangeArgs {
                    timestamp,
                    base_asset: Asset {
                        symbol: base_symbol.to_string(),
                        class: AssetClass::Cryptocurrency,
                    },
                    quote_asset: Asset {
                        symbol: quote_symbol.to_string(),
                        class: AssetClass::Cryptocurrency,
                    },
                },
            )
            .await;
            // Some stablecoin pairs are the inverse (USDT/DAI) of what is desired (DAI/USDT).
            // To ensure USDT is the quote asset, the rate is inverted.
            // If the rate is zero, the rate will be rejected as it will fail to invert.
            if invert {
                result.and_then(|rate| {
                    utils::checked_invert_rate(rate).ok_or(CallExchangeError::NoRatesFound)
                })
            } else {
                result
            }
        };

        Some(future)
    });
    let results = join_all(futures).await;

    let mut rates = vec![];
    let mut errors = vec![];

    for result in results {
        match result {
            Ok(rate) => rates.push(rate),
            Err(error) => {
                ic_cdk::println!(
                    "{} Error while retrieving {} rates @ {}: {}",
                    LOG_PREFIX,
                    symbol,
                    timestamp,
                    error
                );
                errors.push(error);
            }
        }
    }

    if rates.is_empty() {
        return Err(CallExchangeError::NoRatesFound);
    }

    Ok(QueriedExchangeRate::new(
        Asset {
            symbol: symbol.to_string(),
            class: AssetClass::Cryptocurrency,
        },
        Asset {
            symbol: USDT.to_string(),
            class: AssetClass::Cryptocurrency,
        },
        timestamp,
        &rates,
        rates.len() + errors.len(),
        rates.len(),
        None,
    ))
}
