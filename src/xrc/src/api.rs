mod metrics;
#[cfg(test)]
mod test;

pub use metrics::get_metrics;

use crate::{
    call_exchange,
    candid::{Asset, AssetClass, ExchangeRateError, GetExchangeRateRequest, GetExchangeRateResult},
    environment::{CanisterEnvironment, ChargeOption, Environment},
    rate_limiting::{is_rate_limited, with_request_counter},
    stablecoin, utils, with_cache_mut, with_forex_rate_store, CallExchangeArgs, CallExchangeError,
    Exchange, MetricCounter, QueriedExchangeRate, DAI, EXCHANGES, LOG_PREFIX, USD, USDC, USDT,
};
use async_trait::async_trait;
use futures::future::join_all;

/// The expected base rates for stablecoins.
const STABLECOIN_BASES: &[&str] = &[DAI, USDC];

#[async_trait]
trait CallExchanges {
    async fn get_cryptocurrency_usdt_rate(
        &self,
        asset: &Asset,
        timestamp: u64,
    ) -> Result<QueriedExchangeRate, CallExchangeError>;

    async fn get_stablecoin_rates(
        &self,
        symbols: &[&str],
        timestamp: u64,
    ) -> Vec<Result<QueriedExchangeRate, CallExchangeError>>;
}

struct CallExchangesImpl;

#[async_trait]
impl CallExchanges for CallExchangesImpl {
    async fn get_cryptocurrency_usdt_rate(
        &self,
        asset: &Asset,
        timestamp: u64,
    ) -> Result<QueriedExchangeRate, CallExchangeError> {
        let futures = EXCHANGES.iter().filter_map(|exchange| {
            if !exchange.is_available() {
                return None;
            }

            Some(call_exchange(
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

    async fn get_stablecoin_rates(
        &self,
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
}

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
pub async fn get_exchange_rate(request: GetExchangeRateRequest) -> GetExchangeRateResult {
    let env = CanisterEnvironment::new();
    let caller = env.caller();
    let call_exchanges_impl = CallExchangesImpl;

    // Record metrics
    let is_caller_the_cmc = utils::is_caller_the_cmc(&caller);

    MetricCounter::GetExchangeRateRequest.increment();
    if is_caller_the_cmc {
        MetricCounter::GetExchangeRateRequestFromCmc.increment();
    }

    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request).await;

    if result.is_err() {
        MetricCounter::ErrorsReturned.increment();
        if is_caller_the_cmc {
            MetricCounter::ErrorsReturnedToCmc.increment();
        }

        if let Err(ExchangeRateError::NotEnoughCycles) = result {
            MetricCounter::CycleRelatedErrors.increment()
        }
    }

    result
}

async fn get_exchange_rate_internal(
    env: &impl Environment,
    call_exchanges_impl: &impl CallExchanges,
    request: &GetExchangeRateRequest,
) -> GetExchangeRateResult {
    let caller = env.caller();
    if utils::is_caller_anonymous(&caller) {
        return Err(ExchangeRateError::AnonymousPrincipalNotAllowed);
    }

    if !utils::is_caller_the_cmc(&caller) && !env.has_enough_cycles() {
        return Err(ExchangeRateError::NotEnoughCycles);
    }

    let sanitized_request = utils::sanitize_request(request);
    let timestamp = utils::get_normalized_timestamp(env, &sanitized_request);

    // Route the call based on the provided asset types.
    let result = match (
        &sanitized_request.base_asset.class,
        &sanitized_request.quote_asset.class,
    ) {
        (AssetClass::Cryptocurrency, AssetClass::Cryptocurrency) => {
            handle_cryptocurrency_pair(
                env,
                call_exchanges_impl,
                &sanitized_request.base_asset,
                &sanitized_request.quote_asset,
                timestamp,
            )
            .await
        }
        (AssetClass::Cryptocurrency, AssetClass::FiatCurrency) => {
            handle_crypto_base_fiat_quote_pair(
                env,
                call_exchanges_impl,
                &sanitized_request.base_asset,
                &sanitized_request.quote_asset,
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
                env,
                call_exchanges_impl,
                &sanitized_request.quote_asset,
                &sanitized_request.base_asset,
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
        (AssetClass::FiatCurrency, AssetClass::FiatCurrency) => handle_fiat_pair(
            env,
            &sanitized_request.base_asset,
            &sanitized_request.quote_asset,
            timestamp,
        ),
    };

    if let Err(ref error) = result {
        ic_cdk::println!(
            "{} Timestamp: {} Request: {:?} Error: {:?}",
            LOG_PREFIX,
            timestamp,
            sanitized_request,
            error
        );
    }

    // If the result is successful, convert from a `QueriedExchangeRate` to `candid::ExchangeRate`.
    result.map(|r| r.into())
}

/// The function validates the rates in the [QueriedExchangeRate] struct.
fn validate(rate: QueriedExchangeRate) -> Result<QueriedExchangeRate, ExchangeRateError> {
    if rate.is_valid() {
        Ok(rate)
    } else {
        Err(ExchangeRateError::InconsistentRatesReceived)
    }
}

async fn handle_cryptocurrency_pair(
    env: &impl Environment,
    call_exchanges_impl: &impl CallExchanges,
    base_asset: &Asset,
    quote_asset: &Asset,
    timestamp: u64,
) -> Result<QueriedExchangeRate, ExchangeRateError> {
    let caller = env.caller();
    let (maybe_base_rate, maybe_quote_rate) = with_cache_mut(|cache| {
        let maybe_base_rate = cache.get(&(base_asset.symbol.clone(), timestamp)).cloned();
        let maybe_quote_rate = cache.get(&(quote_asset.symbol.clone(), timestamp)).cloned();
        (maybe_base_rate, maybe_quote_rate)
    });

    let mut num_rates_needed: usize = 0;
    if maybe_base_rate.is_none() {
        num_rates_needed = num_rates_needed.saturating_add(1);
    }

    if maybe_quote_rate.is_none() {
        num_rates_needed = num_rates_needed.saturating_add(1);
    }

    if !utils::is_caller_the_cmc(&caller) {
        let rate_limited = is_rate_limited(num_rates_needed);
        let charge_cycles_option = if rate_limited {
            ChargeOption::MinimumFee
        } else {
            ChargeOption::OutboundRatesNeeded(num_rates_needed)
        };
        env.charge_cycles(charge_cycles_option)?;
        if rate_limited {
            return Err(ExchangeRateError::RateLimited);
        }
    }

    // We have all of the necessary rates in the cache return the result.
    if num_rates_needed == 0 {
        return Ok(maybe_base_rate.expect("rate should exist")
            / maybe_quote_rate.expect("rate should exist"));
    }

    let base_asset = base_asset.clone();
    let quote_asset = quote_asset.clone();
    with_request_counter(num_rates_needed, async move {
        let base_rate = match maybe_base_rate {
            Some(base_rate) => base_rate,
            None => {
                let base_rate = call_exchanges_impl
                    .get_cryptocurrency_usdt_rate(&base_asset, timestamp)
                    .await
                    .map_err(|_| ExchangeRateError::CryptoBaseAssetNotFound)?;
                with_cache_mut(|cache| {
                    cache.put((base_asset.symbol.clone(), timestamp), base_rate.clone());
                });
                base_rate
            }
        };

        let quote_rate = match maybe_quote_rate {
            Some(quote_rate) => quote_rate,
            None => {
                let quote_rate = call_exchanges_impl
                    .get_cryptocurrency_usdt_rate(&quote_asset, timestamp)
                    .await
                    .map_err(|_| ExchangeRateError::CryptoQuoteAssetNotFound)?;
                with_cache_mut(|cache| {
                    cache.put((quote_asset.symbol.clone(), timestamp), quote_rate.clone());
                });
                quote_rate
            }
        };

        validate(base_rate / quote_rate)
    })
    .await
}

async fn handle_crypto_base_fiat_quote_pair(
    env: &impl Environment,
    call_exchanges_impl: &impl CallExchanges,
    base_asset: &Asset,
    quote_asset: &Asset,
    timestamp: u64,
) -> Result<QueriedExchangeRate, ExchangeRateError> {
    let caller = env.caller();
    let current_timestamp = env.time_secs();

    let forex_rate_result = with_forex_rate_store(|store| {
        store.get(timestamp, current_timestamp, &quote_asset.symbol, USD)
    })
    .map_err(ExchangeRateError::from);
    let forex_rate = match forex_rate_result {
        Ok(forex_rate) => forex_rate,
        Err(_) => {
            env.charge_cycles(ChargeOption::MinimumFee)?;
            return forex_rate_result;
        }
    };

    let maybe_crypto_base_rate =
        with_cache_mut(|cache| cache.get(&(base_asset.symbol.clone(), timestamp)).cloned());
    let mut num_rates_needed: usize = 0;
    if maybe_crypto_base_rate.is_none() {
        num_rates_needed = num_rates_needed.saturating_add(1);
    }

    // Get stablecoin rates from cache, collecting symbols that were missed.
    let mut missed_stablecoin_symbols = vec![];
    let mut stablecoin_rates = vec![];
    with_cache_mut(|cache| {
        for symbol in STABLECOIN_BASES {
            match cache.get(&(symbol.to_string(), timestamp)) {
                Some(rate) => stablecoin_rates.push(rate.clone()),
                None => missed_stablecoin_symbols.push(*symbol),
            }
        }
    });

    num_rates_needed = num_rates_needed.saturating_add(missed_stablecoin_symbols.len());

    if !utils::is_caller_the_cmc(&caller) {
        let rate_limited = is_rate_limited(num_rates_needed);
        let charge_cycles_option = if rate_limited {
            ChargeOption::MinimumFee
        } else {
            ChargeOption::OutboundRatesNeeded(num_rates_needed)
        };
        env.charge_cycles(charge_cycles_option)?;
        if rate_limited {
            return Err(ExchangeRateError::RateLimited);
        }
    }

    if num_rates_needed == 0 {
        let crypto_base_rate =
            maybe_crypto_base_rate.expect("Crypto base rate should be set here.");
        let stablecoin_rate = stablecoin::get_stablecoin_rate(&stablecoin_rates, &usd_asset())
            .map_err(ExchangeRateError::from)?;
        let crypto_usd_base_rate = crypto_base_rate * stablecoin_rate;
        return Ok(crypto_usd_base_rate / forex_rate);
    }

    let base_asset = base_asset.clone();
    with_request_counter(num_rates_needed, async move {
        // Retrieve the missing stablecoin results. For each rate retrieved, cache it and add it to the
        // stablecoin rates vector.
        let stablecoin_results = call_exchanges_impl
            .get_stablecoin_rates(&missed_stablecoin_symbols, timestamp)
            .await;

        stablecoin_results
            .iter()
            .zip(missed_stablecoin_symbols)
            .for_each(|(result, symbol)| match result {
                Ok(rate) => {
                    stablecoin_rates.push(rate.clone());
                    with_cache_mut(|cache| {
                        cache.put((rate.base_asset.symbol.clone(), timestamp), rate.clone());
                    });
                }
                Err(error) => {
                    ic_cdk::println!(
                        "{} Error while retrieving {} rates @ {}: {}",
                        LOG_PREFIX,
                        symbol,
                        timestamp,
                        error
                    );
                }
            });

        let crypto_base_rate = match maybe_crypto_base_rate {
            Some(base_rate) => base_rate,
            None => {
                let base_rate = call_exchanges_impl
                    .get_cryptocurrency_usdt_rate(&base_asset, timestamp)
                    .await
                    .map_err(|_| ExchangeRateError::CryptoBaseAssetNotFound)?;
                with_cache_mut(|cache| {
                    cache.put(
                        (base_rate.base_asset.symbol.clone(), timestamp),
                        base_rate.clone(),
                    );
                });
                base_rate
            }
        };

        let stablecoin_rate = stablecoin::get_stablecoin_rate(&stablecoin_rates, &usd_asset())
            .map_err(ExchangeRateError::from)?;
        let crypto_usd_base_rate = crypto_base_rate * stablecoin_rate;

        validate(crypto_usd_base_rate / forex_rate)
    })
    .await
}

fn handle_fiat_pair(
    env: &impl Environment,
    base_asset: &Asset,
    quote_asset: &Asset,
    timestamp: u64,
) -> Result<QueriedExchangeRate, ExchangeRateError> {
    let current_timestamp = env.time_secs();
    let result = with_forex_rate_store(|store| {
        store.get(
            timestamp,
            current_timestamp,
            &base_asset.symbol,
            &quote_asset.symbol,
        )
    })
    .map_err(|err| err.into())
    .and_then(validate);

    if !utils::is_caller_the_cmc(&env.caller()) {
        let charge_option = match result {
            Ok(_) => ChargeOption::BaseCost,
            Err(_) => ChargeOption::MinimumFee,
        };

        env.charge_cycles(charge_option)?;
    }

    result
}

async fn get_stablecoin_rate(
    symbol: &str,
    timestamp: u64,
) -> Result<QueriedExchangeRate, CallExchangeError> {
    let mut futures = vec![];
    EXCHANGES.iter().for_each(|exchange| {
        if !cfg!(feature = "ipv4-support") && !exchange.supports_ipv6() {
            return;
        }

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
    if invert {
        result.map(utils::invert_rate)
    } else {
        result
    }
}
