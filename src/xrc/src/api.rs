use crate::{
    call_exchange,
    candid::{Asset, AssetClass, ExchangeRateError, GetExchangeRateRequest, GetExchangeRateResult},
    utils, with_cache_mut, CallExchangeArgs, CallExchangeError, Exchange, QueriedExchangeRate,
    CACHE_RETENTION_PERIOD_SEC, DAI, EXCHANGES, STABLECOIN_CACHE_RETENTION_PERIOD_SEC, USDC, USDT,
};
use futures::future::join_all;
use ic_cdk::export::Principal;

/// The expected base rates for stablecoins.
const STABLECOIN_BASES: &[&str] = &[DAI, USDC];

/// Provides an [Asset] that corresponds to the USDT cryptocurrency stablecoin.
pub fn usdt_asset() -> Asset {
    Asset {
        symbol: USDT.to_string(),
        class: AssetClass::Cryptocurrency,
    }
}

/// This function retrieves the requested rate from the exchanges. The median rate of all collected
/// rates is used as the exchange rate and a set of metadata is returned giving information on
/// how the rate was retrieved.
pub async fn get_exchange_rate(
    caller: Principal,
    request: GetExchangeRateRequest,
) -> GetExchangeRateResult {
    let timestamp = utils::get_normalized_timestamp(&request);

    // Route the call based on the provided asset types.
    let result = match (&request.base_asset.class, &request.quote_asset.class) {
        (AssetClass::Cryptocurrency, AssetClass::Cryptocurrency) => {
            handle_cryptocurrency_pair(
                &caller,
                &request.base_asset,
                &request.quote_asset,
                timestamp,
            )
            .await
        }
        (AssetClass::Cryptocurrency, AssetClass::FiatCurrency) => {
            handle_crypto_base_fiat_quote_pair(
                &caller,
                &request.base_asset,
                &request.quote_asset,
                timestamp,
            )
            .await
        }
        // rustfmt really wants to remove the braces
        #[rustfmt::skip]
        (AssetClass::FiatCurrency, AssetClass::Cryptocurrency) => {
            handle_crypto_base_fiat_quote_pair(
                &caller,
                &request.quote_asset,
                &request.base_asset,
                timestamp,
            )
            .await
            .map(|r| r.inverted())
        },
        (AssetClass::FiatCurrency, AssetClass::FiatCurrency) => {
            handle_fiat_pair(
                &caller,
                &request.base_asset,
                &request.quote_asset,
                timestamp,
            )
            .await
        }
    };

    // If the result is successful, convert from a `QueriedExchangeRate` to `candid::ExchangeRate`.
    result.map(|r| r.into())
}

async fn handle_cryptocurrency_pair(
    caller: &Principal,
    base_asset: &Asset,
    quote_asset: &Asset,
    timestamp: u64,
) -> Result<QueriedExchangeRate, ExchangeRateError> {
    let time = utils::time_secs();

    let (maybe_base_rate, maybe_quote_rate) = with_cache_mut(|mut cache| {
        let maybe_base_rate = cache.get(&base_asset.symbol, timestamp, time);
        // TODO: quote rate should be retrieved from the forex data store.
        let maybe_quote_rate: Option<QueriedExchangeRate> =
            cache.get(&base_asset.symbol, timestamp, time);
        // TODO: Check if stablecoins are in the cache here.
        (maybe_base_rate, maybe_quote_rate)
    });

    let mut num_rates_needed: usize = 0;
    if maybe_base_rate.is_none() {
        num_rates_needed = num_rates_needed.saturating_add(1);
    }

    if maybe_quote_rate.is_none() {
        num_rates_needed = num_rates_needed.saturating_add(1);
    }

    // We have all of the necessary rates in the cache return the result.
    if num_rates_needed == 0 {
        return Ok(maybe_base_rate.expect("rate should exist")
            / maybe_quote_rate.expect("rate should exist"));
    }

    if !utils::is_caller_the_cmc(caller) && !has_capacity() {
        // TODO: replace with variant errors for better clarity
        return Err(ExchangeRateError {
            code: 0,
            description: "Rate limited".to_string(),
        });
    }

    let base_rate = match maybe_base_rate {
        Some(base_rate) => base_rate,
        None => {
            let base_rate = get_cryptocurrency_usdt_rate(base_asset, timestamp).await?;
            with_cache_mut(|mut cache| {
                cache
                    .insert(base_rate.clone(), time, CACHE_RETENTION_PERIOD_SEC)
                    .expect("Inserting into cache should work.");
            });
            base_rate
        }
    };

    let quote_rate = match maybe_quote_rate {
        Some(quote_rate) => quote_rate,
        None => {
            let quote_rate = get_cryptocurrency_usdt_rate(quote_asset, timestamp).await?;
            with_cache_mut(|mut cache| {
                cache
                    .insert(quote_rate.clone(), time, CACHE_RETENTION_PERIOD_SEC)
                    .expect("Inserting into cache should work.");
            });
            quote_rate
        }
    };

    Ok(base_rate / quote_rate)
}

#[allow(unused_variables, unreachable_code, unused_assignments)]
async fn handle_crypto_base_fiat_quote_pair(
    caller: &Principal,
    base_asset: &Asset,
    quote_asset: &Asset,
    timestamp: u64,
) -> Result<QueriedExchangeRate, ExchangeRateError> {
    let time = utils::time_secs();

    let (maybe_base_rate, maybe_quote_rate) = with_cache_mut(|mut cache| {
        let maybe_base_rate = cache.get(&base_asset.symbol, timestamp, time);
        // TODO: quote rate should be retrieved from the forex data store.
        let maybe_quote_rate: Option<QueriedExchangeRate> =
            cache.get(&base_asset.symbol, timestamp, time);
        // TODO: Check if stablecoins are in the cache here.
        (maybe_base_rate, maybe_quote_rate)
    });

    let mut num_rates_needed: usize = 0;
    if maybe_base_rate.is_none() {
        num_rates_needed = num_rates_needed.saturating_add(1);
    }

    if maybe_quote_rate.is_none() {
        num_rates_needed = num_rates_needed.saturating_add(1);
    }

    // If all of the necessary rates are in the cache, return the result.
    if num_rates_needed == 0 {
        return Ok(maybe_base_rate.expect("rate should exist")
            / maybe_quote_rate.expect("rate should exist"));
    }

    // Get stablecoin rates from cache, collecting symbols that were missed.
    let mut missed_stablecoin_symbols = vec![];
    let mut stablecoin_rates = vec![];
    with_cache_mut(|mut cache| {
        for symbol in STABLECOIN_BASES {
            match cache.get(symbol, timestamp, time) {
                Some(rate) => stablecoin_rates.push(rate),
                None => missed_stablecoin_symbols.push(*symbol),
            }
        }
    });

    num_rates_needed = num_rates_needed.saturating_add(missed_stablecoin_symbols.len());

    // Retrieve the missing stablecoin results. For each rate retrieved, cache it and add it to the
    // stablecoin rates vector.
    let stablecoin_results = get_stablecoin_rates(&missed_stablecoin_symbols, timestamp).await;
    // TODO: handle errors that are received in the results
    for rate in stablecoin_results.iter().flatten() {
        stablecoin_rates.push(rate.clone());
        with_cache_mut(|mut cache| {
            cache
                .insert(rate.clone(), time, STABLECOIN_CACHE_RETENTION_PERIOD_SEC)
                .expect("Inserting into the cache should work");
        });
    }

    //stablecoin::get_stablecoin_rate(stablecoin_rates, target);

    todo!()
}

#[allow(unused_variables)]
async fn handle_fiat_pair(
    caller: &Principal,
    base_asset: &Asset,
    quote_asset: &Asset,
    timestamp: u64,
) -> Result<QueriedExchangeRate, ExchangeRateError> {
    todo!()
}

// TODO: replace this function with an actual implementation
fn has_capacity() -> bool {
    true
}

async fn get_cryptocurrency_usdt_rate(
    asset: &Asset,
    timestamp: u64,
) -> Result<QueriedExchangeRate, ExchangeRateError> {
    let results = join_all(EXCHANGES.iter().map(|exchange| {
        call_exchange(
            exchange,
            CallExchangeArgs {
                timestamp,
                quote_asset: usdt_asset(),
                base_asset: asset.clone(),
            },
        )
    }))
    .await;

    let mut rates = vec![];
    let mut errors = vec![];
    for result in results {
        match result {
            Ok(rate) => rates.push(rate),
            Err(err) => errors.push(err),
        }
    }

    // TODO: Handle error case here where rates could be empty from total failure.

    Ok(QueriedExchangeRate::new(
        asset.clone(),
        Asset {
            symbol: USDT.to_string(),
            class: AssetClass::Cryptocurrency,
        },
        timestamp,
        &rates,
        EXCHANGES.len(),
        rates.len(),
    ))
}

#[allow(dead_code)]
async fn get_cryptocurrency_usd_rate(
    asset: &Asset,
    timestamp: u64,
) -> Result<QueriedExchangeRate, ExchangeRateError> {
    let _usdt_rate = get_cryptocurrency_usdt_rate(asset, timestamp).await?;

    // TODO: Convert the rates to USD.
    todo!()
}

#[allow(dead_code)]
async fn get_stablecoin_rates(
    symbols: &[&str],
    timestamp: u64,
) -> Vec<Result<QueriedExchangeRate, CallExchangeError>> {
    join_all(
        symbols
            .iter()
            .map(|symbol| get_stablecoin_rate(symbol, timestamp)),
    )
    .await
}

async fn get_stablecoin_rate(
    symbol: &str,
    timestamp: u64,
) -> Result<QueriedExchangeRate, CallExchangeError> {
    let mut futures = vec![];
    EXCHANGES.iter().for_each(|exchange| {
        let maybe_pair = exchange
            .supported_stablecoin_pairs()
            .iter()
            .find(|pair| pair.0 == symbol || pair.1 == symbol);

        let (base_symbol, quote_symbol) = match maybe_pair {
            Some(pair) => pair,
            None => return,
        };

        let invert = *base_symbol == USDT;

        futures.push(call_exchange_for_stablecoin(
            exchange,
            base_symbol,
            quote_symbol,
            timestamp,
            invert,
        ));
    });

    let results = join_all(futures).await;

    let mut rates = vec![];
    let mut errors = vec![];

    // TODO: if all rates fail, raise error

    for result in results {
        match result {
            Ok(rate) => rates.push(rate),
            Err(error) => errors.push(error),
        }
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
        EXCHANGES.len(),
        rates.len(),
    ))
}

async fn call_exchange_for_stablecoin(
    exchange: &Exchange,
    base_symbol: &str,
    quote_symbol: &str,
    timestamp: u64,
    invert: bool,
) -> Result<u64, CallExchangeError> {
    let result = call_exchange(
        exchange,
        CallExchangeArgs {
            timestamp,
            quote_asset: Asset {
                symbol: base_symbol.to_string(),
                class: AssetClass::Cryptocurrency,
            },
            base_asset: Asset {
                symbol: quote_symbol.to_string(),
                class: AssetClass::Cryptocurrency,
            },
        },
    )
    .await;

    // Some stablecoin pairs are the inverse (USDT/DAI) of what is desired (DAI/USDT).
    // To ensure USDT is the quote asset, the rate is inverted.
    if invert {
        result.map(utils::invert_rate)
    } else {
        result
    }
}
