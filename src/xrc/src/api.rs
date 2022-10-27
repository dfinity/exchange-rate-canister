use crate::{
    call_exchange,
    candid::{Asset, AssetClass, ExchangeRateError, GetExchangeRateRequest, GetExchangeRateResult},
    forex::FOREX_SOURCES,
    rate_limiting::with_rate_limiting,
    stablecoin, utils, with_cache_mut, with_forex_rate_store, CallExchangeArgs, CallExchangeError,
    Exchange, QueriedExchangeRate, CACHE_RETENTION_PERIOD_SEC, DAI, EXCHANGES, LOG_PREFIX,
    STABLECOIN_CACHE_RETENTION_PERIOD_SEC, USD, USDC, USDT,
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

/// Provides an [Asset] that corresponds to the US dollar.
pub fn usd_asset() -> Asset {
    Asset {
        symbol: USD.to_string(),
        class: AssetClass::FiatCurrency,
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
            .map_err(|err| match err {
                ExchangeRateError::ForexBaseAssetNotFound => {
                    ExchangeRateError::ForexQuoteAssetNotFound
                }
                _ => err,
            })
        }
        (AssetClass::FiatCurrency, AssetClass::Cryptocurrency) => {
            handle_crypto_base_fiat_quote_pair(
                &caller,
                &request.quote_asset,
                &request.base_asset,
                timestamp,
            )
            .await
            .map(|r| r.inverted())
            .map_err(|err| match err {
                ExchangeRateError::CryptoBaseAssetNotFound => {
                    ExchangeRateError::CryptoQuoteAssetNotFound
                }
                _ => err,
            })
        }
        (AssetClass::FiatCurrency, AssetClass::FiatCurrency) => {
            handle_fiat_pair(&request.base_asset, &request.quote_asset, timestamp).await
        }
    };

    if let Err(ref error) = result {
        ic_cdk::println!("{} Request: {:?} Error: {:?}", LOG_PREFIX, request, error);
    }

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

    let (maybe_base_rate, maybe_quote_rate) = with_cache_mut(|cache| {
        let maybe_base_rate = cache.get(&base_asset.symbol, timestamp, time);
        let maybe_quote_rate: Option<QueriedExchangeRate> =
            cache.get(&quote_asset.symbol, timestamp, time);
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

    let base_asset = base_asset.clone();
    let quote_asset = quote_asset.clone();
    with_rate_limiting(caller, num_rates_needed, async move {
        let base_rate = match maybe_base_rate {
            Some(base_rate) => base_rate,
            None => {
                let base_rate = get_cryptocurrency_usdt_rate(&base_asset, timestamp)
                    .await
                    .map_err(|_| ExchangeRateError::CryptoBaseAssetNotFound)?;
                with_cache_mut(|cache| {
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
                let quote_rate = get_cryptocurrency_usdt_rate(&quote_asset, timestamp)
                    .await
                    .map_err(|_| ExchangeRateError::CryptoQuoteAssetNotFound)?;
                with_cache_mut(|cache| {
                    cache
                        .insert(quote_rate.clone(), time, CACHE_RETENTION_PERIOD_SEC)
                        .expect("Inserting into cache should work.");
                });
                quote_rate
            }
        };

        Ok(base_rate / quote_rate)
    })
    .await
}

async fn handle_crypto_base_fiat_quote_pair(
    caller: &Principal,
    base_asset: &Asset,
    quote_asset: &Asset,
    timestamp: u64,
) -> Result<QueriedExchangeRate, ExchangeRateError> {
    let time = utils::time_secs();

    let maybe_crypto_base_rate =
        with_cache_mut(|cache| cache.get(&base_asset.symbol, timestamp, time));
    let forex_rate = with_forex_rate_store(|store| store.get(timestamp, &quote_asset.symbol, USD))
        .map(|forex_rate| {
            QueriedExchangeRate::new(
                base_asset.clone(),
                usd_asset(),
                timestamp,
                &[forex_rate.rate],
                if *quote_asset == usd_asset() {
                    FOREX_SOURCES.len()
                } else {
                    0
                },
                forex_rate.num_sources as usize,
            )
        })
        .map_err(ExchangeRateError::from)?;

    let mut num_rates_needed: usize = 0;
    if maybe_crypto_base_rate.is_none() {
        num_rates_needed = num_rates_needed.saturating_add(1);
    }

    // Get stablecoin rates from cache, collecting symbols that were missed.
    let mut missed_stablecoin_symbols = vec![];
    let mut stablecoin_rates = vec![];
    with_cache_mut(|cache| {
        for symbol in STABLECOIN_BASES {
            match cache.get(symbol, timestamp, time) {
                Some(rate) => stablecoin_rates.push(rate),
                None => missed_stablecoin_symbols.push(*symbol),
            }
        }
    });

    num_rates_needed = num_rates_needed.saturating_add(missed_stablecoin_symbols.len());

    let base_asset = base_asset.clone();
    with_rate_limiting(caller, num_rates_needed, async move {
        // Retrieve the missing stablecoin results. For each rate retrieved, cache it and add it to the
        // stablecoin rates vector.
        let stablecoin_results = get_stablecoin_rates(&missed_stablecoin_symbols, timestamp).await;
        // TODO: handle errors that are received in the results
        for rate in stablecoin_results.iter().flatten() {
            stablecoin_rates.push(rate.clone());
            with_cache_mut(|cache| {
                cache
                    .insert(rate.clone(), time, STABLECOIN_CACHE_RETENTION_PERIOD_SEC)
                    .expect("Inserting into the cache should work");
            });
        }

        let stablecoin_rate = stablecoin::get_stablecoin_rate(&stablecoin_rates, &usd_asset())
            .map_err(ExchangeRateError::from)?;

        let crypto_base_rate = match maybe_crypto_base_rate {
            Some(base_rate) => base_rate,
            None => {
                let base_rate = get_cryptocurrency_usdt_rate(&base_asset, timestamp)
                    .await
                    .map_err(|_| ExchangeRateError::CryptoBaseAssetNotFound)?;
                with_cache_mut(|cache| {
                    cache
                        .insert(base_rate.clone(), time, CACHE_RETENTION_PERIOD_SEC)
                        .expect("Inserting into cache should work.");
                });
                base_rate
            }
        };

        let crypto_usd_base_rate = crypto_base_rate * stablecoin_rate;
        Ok(crypto_usd_base_rate / forex_rate)
    })
    .await
}

async fn handle_fiat_pair(
    base_asset: &Asset,
    quote_asset: &Asset,
    timestamp: u64,
) -> Result<QueriedExchangeRate, ExchangeRateError> {
    // TODO: better handling of errors, move to a variant base for ExchangeRateError
    with_forex_rate_store(|store| store.get(timestamp, &base_asset.symbol, &quote_asset.symbol))
        .map(|forex_rate| {
            QueriedExchangeRate::new(
                base_asset.clone(),
                quote_asset.clone(),
                timestamp,
                &[forex_rate.rate],
                if base_asset != quote_asset {
                    FOREX_SOURCES.len()
                } else {
                    0
                },
                forex_rate.num_sources as usize,
            )
        })
        .map_err(|err| err.into())
}

enum GetCryptocurrencyUsdtRateError {
    NoRatesFound,
}

async fn get_cryptocurrency_usdt_rate(
    asset: &Asset,
    timestamp: u64,
) -> Result<QueriedExchangeRate, GetCryptocurrencyUsdtRateError> {
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

    if rates.is_empty() {
        return Err(GetCryptocurrencyUsdtRateError::NoRatesFound);
    }
    // TODO: Handle error case here where rates could be empty from total failure.
    ic_cdk::println!("{:#?}", errors);

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
