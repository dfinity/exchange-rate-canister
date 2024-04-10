mod dashboard;
mod metrics;
#[cfg(test)]
pub(crate) mod test;

pub use dashboard::get_dashboard;
pub use metrics::get_metrics;

use ic_xrc_types::{
    Asset, AssetClass, ExchangeRateError, GetExchangeRateRequest, GetExchangeRateResult,
};

use crate::cache::ExchangeRateCache;
use crate::environment::ChargeCyclesError;
use crate::{
    call_exchange,
    environment::{CanisterEnvironment, ChargeOption, Environment},
    inflight::{is_inflight, with_inflight_tracking},
    rate_limiting::{is_rate_limited, with_request_counter},
    stablecoin, utils, with_cache_mut, with_forex_rate_store, CallExchangeArgs, CallExchangeError,
    Exchange, MetricCounter, QueriedExchangeRate, DAI, DECIMALS, EXCHANGES, LOG_PREFIX,
    ONE_MINUTE_SECONDS, USD, USDC, USDT,
};
use crate::{errors, request_log, NONPRIVILEGED_REQUEST_LOG, PRIVILEGED_REQUEST_LOG};
use async_trait::async_trait;
use candid::Principal;
use futures::future::join_all;

/// The expected base rates for stablecoins.
const STABLECOIN_BASES: &[&str] = &[DAI, USDC];

/// A cached rate is only used for privileged canisters if there are at least this many source rates.
const MIN_NUM_RATES_FOR_PRIVILEGED_CANISTERS: usize =
    if cfg!(feature = "ipv4-support") { 3 } else { 2 };

#[derive(Clone, Debug)]
struct QueriedExchangeRateWithFailedExchanges {
    queried_exchange_rate: QueriedExchangeRate,
    failed_exchanges: Vec<Exchange>,
}

#[async_trait]
trait CallExchanges {
    async fn get_cryptocurrency_usdt_rate(
        &self,
        exchanges: &[&Exchange],
        asset: &Asset,
        timestamp: u64,
    ) -> Result<QueriedExchangeRateWithFailedExchanges, CallExchangeError>;

    async fn get_stablecoin_rates(
        &self,
        exchanges: &[&Exchange],
        symbols: &[&str],
        timestamp: u64,
    ) -> Vec<Result<QueriedExchangeRateWithFailedExchanges, CallExchangeError>>;
}

struct CallExchangesImpl;

#[async_trait]
impl CallExchanges for CallExchangesImpl {
    async fn get_cryptocurrency_usdt_rate(
      &self,
      exchanges: &[&Exchange],
      asset: &Asset,
      timestamp: u64,
    ) -> Result<QueriedExchangeRateWithFailedExchanges, CallExchangeError> {
      let futures = exchanges.iter().map(|exchange| {
          call_exchange(
              exchange,
              CallExchangeArgs {
                  timestamp,
                  quote_asset: usdt_asset(),
                  base_asset: asset.clone(),
              },
          )
      });
      let results = join_all(futures).await;

      let mut rates = vec![];
      let mut failed_exchanges = vec![];
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

                  if let CallExchangeError::Http { exchange, error: _} = err {
                      if let Some(exchange) = exchanges.iter().find(|e| e.to_string() == exchange) {
                          failed_exchanges.push((*exchange).clone());
                      }
                      else {
                          ic_cdk::println!("{} Exchange not found for failed exchanges: {} @ {}", LOG_PREFIX, exchange, timestamp);
                      }
                  } 
              }
          }
      }

      if rates.is_empty() {
          return Err(CallExchangeError::NoRatesFound);
      }

      Ok(QueriedExchangeRateWithFailedExchanges {
          queried_exchange_rate: QueriedExchangeRate::new(
              asset.clone(),
              usdt_asset(),
              timestamp,
              &rates,
              exchanges.len(),
              rates.len(),
              None,
          ),
          failed_exchanges,
      })
    }

    async fn get_stablecoin_rates(
        &self,
        exchanges: &[&Exchange],
        symbols: &[&str],
        timestamp: u64,
    ) -> Vec<Result<QueriedExchangeRateWithFailedExchanges, CallExchangeError>> {
        join_all(
            symbols
                .iter()
                .map(|symbol| get_stablecoin_rate(exchanges, symbol, timestamp)),
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
    let timestamp = env.time_secs();
    let caller = env.caller();
    let call_exchanges_impl = CallExchangesImpl;

    // Record metrics
    let is_caller_privileged = utils::is_caller_privileged(&caller);

    MetricCounter::GetExchangeRateRequest.increment();
    if is_caller_privileged {
        MetricCounter::GetExchangeRateRequestFromCmc.increment();
    }

    let result = get_exchange_rate_internal(&env, &call_exchanges_impl, &request).await;

    if is_caller_privileged {
        request_log::log(
            &PRIVILEGED_REQUEST_LOG,
            &caller,
            timestamp,
            &request,
            &result,
        );
    } else {
        request_log::log(
            &NONPRIVILEGED_REQUEST_LOG,
            &caller,
            timestamp,
            &request,
            &result,
        );
    }

    if let Err(ref error) = result {
        MetricCounter::ErrorsReturned.increment();

        if is_caller_privileged {
            MetricCounter::ErrorsReturnedToCmc.increment();
        }

        match error {
            ExchangeRateError::Pending => MetricCounter::PendingErrorsReturned.increment(),
            ExchangeRateError::CryptoBaseAssetNotFound
            | ExchangeRateError::CryptoQuoteAssetNotFound => {
                MetricCounter::CryptoAssetRelatedErrorsReturned.increment()
            }
            ExchangeRateError::StablecoinRateNotFound
            | ExchangeRateError::StablecoinRateTooFewRates
            | ExchangeRateError::StablecoinRateZeroRate => {
                MetricCounter::StablecoinErrorsReturned.increment()
            }
            ExchangeRateError::ForexInvalidTimestamp
            | ExchangeRateError::ForexBaseAssetNotFound
            | ExchangeRateError::ForexQuoteAssetNotFound
            | ExchangeRateError::ForexAssetsNotFound => {
                MetricCounter::ForexAssetRelatedErrorsReturned.increment()
            }
            ExchangeRateError::RateLimited => MetricCounter::RateLimitedErrors.increment(),
            ExchangeRateError::NotEnoughCycles => MetricCounter::CycleRelatedErrors.increment(),
            ExchangeRateError::InconsistentRatesReceived => {
                MetricCounter::InconsistentRatesErrorsReturned.increment()
            }
            ExchangeRateError::AnonymousPrincipalNotAllowed | ExchangeRateError::Other(_) => {}
        };
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
    let result = route_request(env, call_exchanges_impl, &sanitized_request).await;

    if let Err(ref error) = result {
        let timestamp = utils::get_normalized_timestamp(env, &sanitized_request);
        ic_cdk::println!(
            "{} Caller: {} Timestamp: {} Request: {:?} Error: {:?}",
            LOG_PREFIX,
            caller,
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

/// This function is used for inverted errors returned when handling fiat-crypto pairs.
fn invert_exchange_rate_error_for_fiat_crypto_pair(error: ExchangeRateError) -> ExchangeRateError {
    match error {
        ExchangeRateError::CryptoBaseAssetNotFound => ExchangeRateError::CryptoQuoteAssetNotFound,
        ExchangeRateError::ForexQuoteAssetNotFound => ExchangeRateError::ForexBaseAssetNotFound,
        ExchangeRateError::Other(ref other_error) => {
            if other_error.code == errors::BASE_ASSET_INVALID_SYMBOL_ERROR_CODE {
                errors::quote_asset_symbol_invalid_error()
            } else if other_error.code == errors::QUOTE_ASSET_INVALID_SYMBOL_ERROR_CODE {
                errors::base_asset_symbol_invalid_error()
            } else {
                error
            }
        }
        _ => error,
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
                .map(|rate| rate.inverted())
                .map_err(invert_exchange_rate_error_for_fiat_crypto_pair)
        }
        (AssetClass::FiatCurrency, AssetClass::FiatCurrency) => handle_fiat_pair(env, request),
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
        NormalizedTimestamp::past(timestamp.saturating_sub(ONE_MINUTE_SECONDS))
    } else {
        NormalizedTimestamp::requested_or_current(timestamp)
    }
}

/// This function extracts the exchange rate for the given symbol and timestamp from the cache.
fn get_rate_from_cache(
    cache: &mut ExchangeRateCache,
    caller: &Principal,
    symbol: &str,
    timestamp: u64,
) -> Option<QueriedExchangeRate> {
    let maybe_rate = cache.get(symbol, timestamp);
    if !utils::is_caller_privileged(caller) {
        return maybe_rate;
    }
    match maybe_rate {
        Some(ref rate) => {
            if rate.base_asset.symbol == USDT
                || rate.rates.len() >= MIN_NUM_RATES_FOR_PRIVILEGED_CANISTERS
            {
                maybe_rate
            } else {
                None
            }
        }
        None => None,
    }
}

/// The possible errors that [validate_request] may return if a request
/// fails the validation.
enum ValidateRequestError {
    /// The timestamp is in the future.
    FutureTimestamp {
        /// The normalized requested timestamp.
        requested_timestamp: u64,
        /// The current IC time in seconds.
        current_timestamp: u64,
    },
    /// The request is hitting the rate limit.
    RateLimited,
    /// The request already has outbound HTTP calls.
    AlreadyInflight,
    /// The request timestamp that goes back in the past is not in the rate cache.
    PastTimestampNotCached,
    /// The base asset symbol provided contains invalid characters.
    BaseAssetInvalidSymbol,
    /// The quote asset symbol provided contains invalid characters.
    QuoteAssetInvalidSymbol,
}

impl From<ValidateRequestError> for ExchangeRateError {
    fn from(error: ValidateRequestError) -> Self {
        match error {
            ValidateRequestError::FutureTimestamp {
                requested_timestamp,
                current_timestamp,
            } => errors::timestamp_is_in_future_error(requested_timestamp, current_timestamp),
            ValidateRequestError::RateLimited => ExchangeRateError::RateLimited,
            ValidateRequestError::AlreadyInflight => ExchangeRateError::Pending,
            ValidateRequestError::PastTimestampNotCached => ExchangeRateError::Pending,
            ValidateRequestError::BaseAssetInvalidSymbol => {
                errors::base_asset_symbol_invalid_error()
            }
            ValidateRequestError::QuoteAssetInvalidSymbol => {
                errors::quote_asset_symbol_invalid_error()
            }
        }
    }
}

/// This function validates a santized request with the given number of rates needed
/// in order to complete the request.
fn validate_request(
    env: &impl Environment,
    request: &GetExchangeRateRequest,
    num_rates_needed: usize,
    requested_timestamp: &NormalizedTimestamp,
) -> Result<(), ValidateRequestError> {
    let current_timestamp = env.time_secs();
    if requested_timestamp.value > current_timestamp {
        return Err(ValidateRequestError::FutureTimestamp {
            current_timestamp,
            requested_timestamp: requested_timestamp.value,
        });
    }

    if request.base_asset.symbol.is_empty() {
        return Err(ValidateRequestError::BaseAssetInvalidSymbol);
    }

    if request.quote_asset.symbol.is_empty() {
        return Err(ValidateRequestError::QuoteAssetInvalidSymbol);
    }

    if utils::is_caller_privileged(&env.caller()) {
        return Ok(());
    }

    if is_rate_limited(num_rates_needed) {
        Err(ValidateRequestError::RateLimited)
    } else if (request.base_asset.class == AssetClass::Cryptocurrency
        && is_inflight(&request.base_asset, requested_timestamp.value))
        || (request.quote_asset.class == AssetClass::Cryptocurrency
            && is_inflight(&request.quote_asset, requested_timestamp.value))
    {
        Err(ValidateRequestError::AlreadyInflight)
    } else if requested_timestamp.r#type == NormalizedTimestampType::Past && num_rates_needed > 0 {
        Err(ValidateRequestError::PastTimestampNotCached)
    } else {
        Ok(())
    }
}

/// This function attempts to charge cycles for the request being made.
/// If an error is found validating the request, charge the minimum fee.
/// Otherwise, charge what is necessary for the number of needed outbound calls.
/// If the caller is privileged, exit early as they are not charged cycles.
fn charge_cycles(
    env: &impl Environment,
    num_rates_needed: usize,
    is_valid_request: bool,
) -> Result<(), ChargeCyclesError> {
    if utils::is_caller_privileged(&env.caller()) {
        return Ok(());
    }

    let charge_cycles_option = if is_valid_request {
        ChargeOption::OutboundRatesNeeded(num_rates_needed)
    } else {
        ChargeOption::MinimumFee
    };

    env.charge_cycles(charge_cycles_option)
}

async fn handle_cryptocurrency_pair(
    env: &impl Environment,
    call_exchanges_impl: &impl CallExchanges,
    request: &GetExchangeRateRequest
) -> Result<QueriedExchangeRate, ExchangeRateError> {
    let requested_timestamp = get_normalized_timestamp(env, request);
    let mut failed_exchanges = vec![];
    let mut exchanges = EXCHANGES
        .iter()
        .filter(|exchange| exchange.is_available())
        .collect::<Vec<_>>();
    let caller = env.caller();
    let (maybe_base_rate, maybe_quote_rate) = with_cache_mut(|cache| {
        (
            get_rate_from_cache(
                cache,
                &caller,
                &request.base_asset.symbol,
                requested_timestamp.value,
            ),
            get_rate_from_cache(
                cache,
                &caller,
                &request.quote_asset.symbol,
                requested_timestamp.value,
            ),
        )
    });

    let mut num_rates_needed: usize = 0;
    if maybe_base_rate.is_none() {
        num_rates_needed = num_rates_needed.saturating_add(1);
    }

    if maybe_quote_rate.is_none() {
        num_rates_needed = num_rates_needed.saturating_add(1);
    }

    let validate_request_result =
        validate_request(env, request, num_rates_needed, &requested_timestamp);
    charge_cycles(env, num_rates_needed, validate_request_result.is_ok())?;

    if let Err(error) = validate_request_result {
        return Err(error.into());
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
        requested_timestamp.value,
        with_request_counter(num_rates_needed, async move {
            let base_rate = match maybe_base_rate {
                Some(base_rate) => base_rate,
                None => {
                    let response = call_exchanges_impl
                        .get_cryptocurrency_usdt_rate(
                            &exchanges,
                            &request.base_asset,
                            requested_timestamp.value,
                        )
                        .await
                        .map_err(|_| ExchangeRateError::CryptoBaseAssetNotFound)?;
                    with_cache_mut(|cache| {
                        cache.insert(&response.queried_exchange_rate);
                    });
                    failed_exchanges.extend(response.failed_exchanges);
                    response.queried_exchange_rate
                }
            };
            exchanges.retain(|exchange| !failed_exchanges.contains(exchange));

            let quote_rate = match maybe_quote_rate {
                Some(quote_rate) => quote_rate,
                None => {
                    let response = call_exchanges_impl
                        .get_cryptocurrency_usdt_rate(
                            &exchanges,
                            &request.quote_asset,
                            requested_timestamp.value,
                        )
                        .await
                        .map_err(|_| ExchangeRateError::CryptoQuoteAssetNotFound)?;
                    with_cache_mut(|cache| {
                        cache.insert(&response.queried_exchange_rate);
                    });
                    failed_exchanges.extend(response.failed_exchanges);
                    response.queried_exchange_rate
                }
            };
            (base_rate / quote_rate).validate()
        }),
    )
    .await
}

async fn handle_crypto_base_fiat_quote_pair(
    env: &impl Environment,
    call_exchanges_impl: &impl CallExchanges,
    request: &GetExchangeRateRequest,
) -> Result<QueriedExchangeRate, ExchangeRateError> {
    let requested_timestamp = get_normalized_timestamp(env, request);
    let caller = env.caller();
    let mut failed_exchanges_list = vec![];
    let mut exchanges = EXCHANGES
        .iter()
        .filter(|exchange| exchange.is_available())
        .collect::<Vec<_>>();
    let maybe_crypto_base_rate = with_cache_mut(|cache| {
        get_rate_from_cache(
            cache,
            &caller,
            &request.base_asset.symbol,
            requested_timestamp.value,
        )
    });
    let mut num_rates_needed: usize = 0;
    if maybe_crypto_base_rate.is_none() {
        num_rates_needed = num_rates_needed.saturating_add(1);
    }

    // Get stablecoin rates from cache, collecting symbols that were missed.
    let mut missed_stablecoin_symbols = vec![];
    let mut stablecoin_rates = vec![];
    with_cache_mut(|cache| {
        for symbol in STABLECOIN_BASES {
            match cache.get(symbol, requested_timestamp.value) {
                Some(rate) => stablecoin_rates.push(rate.clone()),
                None => missed_stablecoin_symbols.push(*symbol),
            }
        }
    });

    num_rates_needed = num_rates_needed.saturating_add(missed_stablecoin_symbols.len());

    let validate_request_result =
        validate_request(env, request, num_rates_needed, &requested_timestamp);

    let forex_rate_result = with_forex_rate_store(|store| {
        let current_timestamp_secs = env.time_secs();
        store.get(
            requested_timestamp.value,
            current_timestamp_secs,
            &request.quote_asset.symbol,
            USD,
        )
    })
    .map_err(ExchangeRateError::from);

    charge_cycles(
        env,
        num_rates_needed,
        validate_request_result.is_ok() && forex_rate_result.is_ok(),
    )?;

    if let Err(error) = validate_request_result {
        return Err(error.into());
    }
    let forex_rate = forex_rate_result?;

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
        requested_timestamp.value,
        with_request_counter(num_rates_needed, async move {
            // Retrieve the missing stablecoin results. For each rate retrieved, cache it and add it to the
            // stablecoin rates vector.
            let stablecoin_results = call_exchanges_impl
                .get_stablecoin_rates(&exchanges, &missed_stablecoin_symbols, requested_timestamp.value)
                .await;

            stablecoin_results
                .into_iter()
                .zip(missed_stablecoin_symbols)
                .for_each(|(result, symbol)| match result {
                    Ok(QueriedExchangeRateWithFailedExchanges {
                        failed_exchanges,
                        queried_exchange_rate,
                    }) => {
                        failed_exchanges_list.extend(failed_exchanges);
                        stablecoin_rates.push(queried_exchange_rate.clone());
                        with_cache_mut(|cache| {
                            cache.insert(&queried_exchange_rate);
                        });
                    }
                    Err(error) => {
                        ic_cdk::println!(
                            "{} Error while retrieving {} rates @ {}: {}",
                            LOG_PREFIX,
                            symbol,
                            requested_timestamp.value,
                            error
                        );
                    }
                });

            exchanges.retain(|exchange| !failed_exchanges_list.contains(exchange));
            let crypto_base_rate = match maybe_crypto_base_rate {
                Some(base_rate) => base_rate,
                None => {
                    let response = call_exchanges_impl
                        .get_cryptocurrency_usdt_rate(
                            &exchanges,
                            &request.base_asset,
                            requested_timestamp.value,
                        )
                        .await
                        .map_err(|_| ExchangeRateError::CryptoBaseAssetNotFound)?;
                    with_cache_mut(|cache| {
                        cache.insert(&response.queried_exchange_rate);
                    });
                    failed_exchanges_list.extend(response.failed_exchanges);
                    response.queried_exchange_rate
                }
            };

            let stablecoin_rate = stablecoin::get_stablecoin_rate(&stablecoin_rates, &usd_asset())
                .map_err(ExchangeRateError::from)?;
            let crypto_usd_base_rate = crypto_base_rate * stablecoin_rate;
            (crypto_usd_base_rate / forex_rate).validate()
        }),
    )
    .await
}

fn handle_fiat_pair(
    env: &impl Environment,
    request: &GetExchangeRateRequest,
) -> Result<QueriedExchangeRate, ExchangeRateError> {
    let requested_timestamp =
        NormalizedTimestamp::requested_or_current(utils::get_normalized_timestamp(env, request));
    let current_timestamp = env.time_secs();
    let validate_result = validate_request(env, request, 0, &requested_timestamp);
    let result = match validate_result {
        Ok(_) => with_forex_rate_store(|store| {
            store.get(
                requested_timestamp.value,
                current_timestamp,
                &request.base_asset.symbol,
                &request.quote_asset.symbol,
            )
        })
        .map_err(|err| err.into())
        .and_then(QueriedExchangeRate::validate),
        Err(error) => return Err(error.into()),
    };

    charge_cycles(env, 0, result.is_ok())?;

    result
}

async fn get_stablecoin_rate(
    exchanges: &[&Exchange],
    symbol: &str,
    timestamp: u64,
) -> Result<QueriedExchangeRateWithFailedExchanges, CallExchangeError> {
    let mut futures = vec![];
    exchanges.iter().for_each(|exchange| {
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
    let mut failed_exchanges = vec![];
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
                if let CallExchangeError::Http { exchange, error: _} = error {
                    if let Some(exchange) = exchanges.iter().find(|e| e.to_string() == exchange) {
                        failed_exchanges.push((*exchange).clone());
                    }
                    else {
                        ic_cdk::println!("{} Exchange not found for failed exchanges: {} @ {}", LOG_PREFIX, exchange, timestamp);
                    }
                } 
            }
        }
    }

    if rates.is_empty() {
        return Err(CallExchangeError::NoRatesFound);
    }

    Ok(QueriedExchangeRateWithFailedExchanges {
      queried_exchange_rate: QueriedExchangeRate::new(
          Asset {
            symbol: symbol.to_string(),
            class: AssetClass::Cryptocurrency,
          },
          usdt_asset(),
          timestamp,
          &rates,
          exchanges.len(),
          rates.len(),
          None,
      ),
      failed_exchanges,
  })
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
    // If the rate is zero, the rate will be rejected as it will fail to invert.
    if invert {
        result.and_then(|rate| {
            utils::checked_invert_rate(rate.into(), DECIMALS).ok_or(CallExchangeError::NoRatesFound)
        })
    } else {
        result
    }
}
