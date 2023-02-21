mod metrics;
#[cfg(test)]
mod test;

pub use metrics::get_metrics;

use crate::{
    call_exchange,
    candid::{Asset, AssetClass, ExchangeRateError, GetExchangeRateRequest, GetExchangeRateResult},
    environment::{CanisterEnvironment, ChargeOption, Environment},
    inflight::{is_inflight, with_inflight_tracking},
    rate_limiting::{is_rate_limited, with_request_counter},
    stablecoin, utils, with_cache_mut, with_forex_rate_store, CallExchangeArgs, CallExchangeError,
    Exchange, MetricCounter, QueriedExchangeRate, DAI, EXCHANGES, LOG_PREFIX, ONE_MINUTE, USD,
    USDC, USDT,
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
    let is_caller_privileged = utils::is_caller_privileged(&caller);

    MetricCounter::GetExchangeRateRequest.increment();
    if is_caller_privileged {
        MetricCounter::GetExchangeRateRequestFromCmc.increment();
    }

    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request).await;

    if result.is_err() {
        MetricCounter::ErrorsReturned.increment();
        if is_caller_privileged {
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

    if !utils::is_caller_privileged(&caller) && !env.has_enough_cycles() {
        return Err(ExchangeRateError::NotEnoughCycles);
    }

    let sanitized_request = utils::sanitize_request(request);
    // Route the call based on the provided asset types.
    let result = route_request(env, call_exchanges_impl, request).await;

    if let Err(ref error) = result {
        let timestamp = utils::get_normalized_timestamp(env, &sanitized_request);
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

/// This function is used for handling fiat-crypto pairs.
fn invert_assets_in_request(request: &GetExchangeRateRequest) -> GetExchangeRateRequest {
    GetExchangeRateRequest {
        base_asset: request.quote_asset.clone(),
        quote_asset: request.base_asset.clone(),
        timestamp: request.timestamp,
    }
}

/// This function routes a request to the appropriate handler by lookin gat the asset classes.
async fn route_request(
    env: &impl Environment,
    call_exchanges_impl: &impl CallExchanges,
    request: &GetExchangeRateRequest,
) -> Result<QueriedExchangeRate, ExchangeRateError> {
    match (&request.base_asset.class, &request.quote_asset.class) {
        (AssetClass::Cryptocurrency, AssetClass::Cryptocurrency) => {
            handle_cryptocurrency_pair(env, call_exchanges_impl, request).await
        }
        (AssetClass::Cryptocurrency, AssetClass::FiatCurrency) => {
            handle_crypto_base_fiat_quote_pair(env, call_exchanges_impl, request)
                .await
                .map_err(|err| match err {
                    ExchangeRateError::ForexBaseAssetNotFound => {
                        ExchangeRateError::ForexQuoteAssetNotFound
                    }
                    _ => err,
                })
        }
        (AssetClass::FiatCurrency, AssetClass::Cryptocurrency) => {
            let inverted_request = invert_assets_in_request(request);
            handle_crypto_base_fiat_quote_pair(env, call_exchanges_impl, &inverted_request)
                .await
                .map(|r| r.inverted())
                .map_err(|err| match err {
                    ExchangeRateError::CryptoBaseAssetNotFound => {
                        ExchangeRateError::CryptoQuoteAssetNotFound
                    }
                    _ => err,
                })
        }
        (AssetClass::FiatCurrency, AssetClass::FiatCurrency) => handle_fiat_pair(env, request),
    }
}

/// The function validates the rates in the [QueriedExchangeRate] struct.
fn validate(rate: QueriedExchangeRate) -> Result<QueriedExchangeRate, ExchangeRateError> {
    if rate.is_valid() {
        Ok(rate)
    } else {
        Err(ExchangeRateError::InconsistentRatesReceived)
    }
}

#[derive(Debug, PartialEq, Eq)]
enum NormalizedTimestampType {
    /// The timestamp is within the past minute.
    RequestedOrCurrent,
    /// The timestamp is from the past.
    Past,
}

#[derive(Debug)]
struct NormalizedTimestamp {
    /// The timestamp in seconds.
    value: u64,
    /// Used to determine if the timestamp is a current timestamp or a timestamp from a minute ago.
    r#type: NormalizedTimestampType,
}

impl NormalizedTimestamp {
    /// Creates a new timestamp with the type `RequestedOrCurrent`
    fn requested_or_current(value: u64) -> Self {
        Self {
            value,
            r#type: NormalizedTimestampType::RequestedOrCurrent,
        }
    }

    /// Creates a new timestamp with the type `Past`
    fn past(value: u64) -> Self {
        Self {
            value,
            r#type: NormalizedTimestampType::Past,
        }
    }
}

/// If the request contains a timestamp, the function returns the normalized requested timestamp.
/// If the request's timestamp is null, the function returns the current timestamp if no assets are found
/// to be inflight; otherwise, the normalized timestamp from one minute ago.
fn get_normalized_timestamp(
    env: &impl Environment,
    request: &GetExchangeRateRequest,
) -> NormalizedTimestamp {
    let timestamp = utils::get_normalized_timestamp(env, request);
    if request.timestamp.is_some() {
        return NormalizedTimestamp::requested_or_current(timestamp);
    }

    if is_inflight(&request.base_asset, timestamp) || is_inflight(&request.quote_asset, timestamp) {
        NormalizedTimestamp::past(timestamp.saturating_sub(ONE_MINUTE))
    } else {
        NormalizedTimestamp::requested_or_current(timestamp)
    }
}

async fn handle_cryptocurrency_pair(
    env: &impl Environment,
    call_exchanges_impl: &impl CallExchanges,
    request: &GetExchangeRateRequest,
) -> Result<QueriedExchangeRate, ExchangeRateError> {
    let timestamp = get_normalized_timestamp(env, request);

    let caller = env.caller();
    let (maybe_base_rate, maybe_quote_rate) = with_cache_mut(|cache| {
        let maybe_base_rate = cache.get(&request.base_asset.symbol, timestamp.value);
        let maybe_quote_rate = cache.get(&request.quote_asset.symbol, timestamp.value);
        (maybe_base_rate, maybe_quote_rate)
    });

    let mut num_rates_needed: usize = 0;
    if maybe_base_rate.is_none() {
        num_rates_needed = num_rates_needed.saturating_add(1);
    }

    if maybe_quote_rate.is_none() {
        num_rates_needed = num_rates_needed.saturating_add(1);
    }

    if !utils::is_caller_privileged(&caller) {
        let rate_limited = is_rate_limited(num_rates_needed);
        let already_inflight = is_inflight(&request.base_asset, timestamp.value)
            || is_inflight(&request.quote_asset, timestamp.value);
        let is_past_timestamp_not_cached =
            timestamp.r#type == NormalizedTimestampType::Past && num_rates_needed > 0;
        let charge_cycles_option =
            if rate_limited || already_inflight || is_past_timestamp_not_cached {
                ChargeOption::MinimumFee
            } else {
                ChargeOption::OutboundRatesNeeded(num_rates_needed)
            };

        env.charge_cycles(charge_cycles_option)?;
        if rate_limited {
            return Err(ExchangeRateError::RateLimited);
        }

        if already_inflight || is_past_timestamp_not_cached {
            return Err(ExchangeRateError::Pending);
        }
    }

    // We have all of the necessary rates in the cache return the result.
    if num_rates_needed == 0 {
        return Ok(maybe_base_rate.expect("rate should exist")
            / maybe_quote_rate.expect("rate should exist"));
    }

    with_inflight_tracking(
        vec![
            request.base_asset.symbol.clone(),
            request.quote_asset.symbol.clone(),
        ],
        timestamp.value,
        with_request_counter(num_rates_needed, async move {
            let base_rate = match maybe_base_rate {
                Some(base_rate) => base_rate,
                None => {
                    let base_rate = call_exchanges_impl
                        .get_cryptocurrency_usdt_rate(&request.base_asset, timestamp.value)
                        .await
                        .map_err(|_| ExchangeRateError::CryptoBaseAssetNotFound)?;
                    with_cache_mut(|cache| {
                        cache.insert(&base_rate);
                    });
                    base_rate
                }
            };

            let quote_rate = match maybe_quote_rate {
                Some(quote_rate) => quote_rate,
                None => {
                    let quote_rate = call_exchanges_impl
                        .get_cryptocurrency_usdt_rate(&request.quote_asset, timestamp.value)
                        .await
                        .map_err(|_| ExchangeRateError::CryptoQuoteAssetNotFound)?;
                    with_cache_mut(|cache| {
                        cache.insert(&quote_rate);
                    });
                    quote_rate
                }
            };

            validate(base_rate / quote_rate)
        }),
    )
    .await
}

async fn handle_crypto_base_fiat_quote_pair(
    env: &impl Environment,
    call_exchanges_impl: &impl CallExchanges,
    request: &GetExchangeRateRequest,
) -> Result<QueriedExchangeRate, ExchangeRateError> {
    let timestamp = get_normalized_timestamp(env, request);
    let caller = env.caller();

    let forex_rate_result = with_forex_rate_store(|store| {
        let current_timestamp_secs = env.time_secs();
        store.get(
            timestamp.value,
            current_timestamp_secs,
            &request.quote_asset.symbol,
            USD,
        )
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
        with_cache_mut(|cache| cache.get(&request.base_asset.symbol, timestamp.value));
    let mut num_rates_needed: usize = 0;
    if maybe_crypto_base_rate.is_none() {
        num_rates_needed = num_rates_needed.saturating_add(1);
    }

    // Get stablecoin rates from cache, collecting symbols that were missed.
    let mut missed_stablecoin_symbols = vec![];
    let mut stablecoin_rates = vec![];
    with_cache_mut(|cache| {
        for symbol in STABLECOIN_BASES {
            match cache.get(symbol, timestamp.value) {
                Some(rate) => stablecoin_rates.push(rate.clone()),
                None => missed_stablecoin_symbols.push(*symbol),
            }
        }
    });

    num_rates_needed = num_rates_needed.saturating_add(missed_stablecoin_symbols.len());

    if !utils::is_caller_privileged(&caller) {
        let rate_limited = is_rate_limited(num_rates_needed);
        let already_inflight = is_inflight(&request.base_asset, timestamp.value);
        let is_past_minute_not_cached =
            timestamp.r#type == NormalizedTimestampType::Past && num_rates_needed > 0;
        let charge_cycles_option = if rate_limited || already_inflight || is_past_minute_not_cached
        {
            ChargeOption::MinimumFee
        } else {
            ChargeOption::OutboundRatesNeeded(num_rates_needed)
        };

        env.charge_cycles(charge_cycles_option)?;
        if rate_limited {
            return Err(ExchangeRateError::RateLimited);
        }

        if already_inflight || is_past_minute_not_cached {
            return Err(ExchangeRateError::Pending);
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

    with_inflight_tracking(
        vec![request.base_asset.symbol.clone()],
        timestamp.value,
        with_request_counter(num_rates_needed, async move {
            // Retrieve the missing stablecoin results. For each rate retrieved, cache it and add it to the
            // stablecoin rates vector.
            let stablecoin_results = call_exchanges_impl
                .get_stablecoin_rates(&missed_stablecoin_symbols, timestamp.value)
                .await;

            stablecoin_results
                .iter()
                .zip(missed_stablecoin_symbols)
                .for_each(|(result, symbol)| match result {
                    Ok(rate) => {
                        stablecoin_rates.push(rate.clone());
                        with_cache_mut(|cache| {
                            cache.insert(rate);
                        });
                    }
                    Err(error) => {
                        ic_cdk::println!(
                            "{} Error while retrieving {} rates @ {}: {}",
                            LOG_PREFIX,
                            symbol,
                            timestamp.value,
                            error
                        );
                    }
                });

            let crypto_base_rate = match maybe_crypto_base_rate {
                Some(base_rate) => base_rate,
                None => {
                    let base_rate = call_exchanges_impl
                        .get_cryptocurrency_usdt_rate(&request.base_asset, timestamp.value)
                        .await
                        .map_err(|_| ExchangeRateError::CryptoBaseAssetNotFound)?;
                    with_cache_mut(|cache| {
                        cache.insert(&base_rate);
                    });
                    base_rate
                }
            };

            let stablecoin_rate = stablecoin::get_stablecoin_rate(&stablecoin_rates, &usd_asset())
                .map_err(ExchangeRateError::from)?;
            let crypto_usd_base_rate = crypto_base_rate * stablecoin_rate;

            validate(crypto_usd_base_rate / forex_rate)
        }),
    )
    .await
}

fn handle_fiat_pair(
    env: &impl Environment,
    request: &GetExchangeRateRequest,
) -> Result<QueriedExchangeRate, ExchangeRateError> {
    let timestamp = utils::get_normalized_timestamp(env, request);
    let current_timestamp = env.time_secs();
    let result = with_forex_rate_store(|store| {
        store.get(
            timestamp,
            current_timestamp,
            &request.base_asset.symbol,
            &request.quote_asset.symbol,
        )
    })
    .map_err(|err| err.into())
    .and_then(validate);

    if !utils::is_caller_privileged(&env.caller()) {
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
