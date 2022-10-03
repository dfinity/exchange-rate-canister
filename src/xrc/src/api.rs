use crate::{
    call_exchange,
    candid::{Asset, AssetClass, ExchangeRateError, GetExchangeRateRequest, GetExchangeRateResult},
    utils, with_cache_mut, CallExchangesArgs, QueriedExchangeRate, EXCHANGES, EXCHANGE_RATE_CACHE,
};
use futures::future::join_all;
use ic_cdk::export::Principal;

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
        let maybe_quote_rate = cache.get(&quote_asset.symbol, timestamp, time);
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

    // TODO: get missing stablecoin rates
    //stablecoin::get_stablecoin_rate(stablecoin_rates, target);
    let base_rate = match maybe_base_rate {
        Some(base_rate) => base_rate,
        None => {
            let base_rate = get_cryptocurrency_usd_rate(base_asset, timestamp).await?;
            with_cache_mut(|mut cache| {
                cache.insert(base_rate.clone(), time);
            });
            base_rate
        }
    };

    let quote_rate = match maybe_quote_rate {
        Some(quote_rate) => quote_rate,
        None => {
            let quote_rate = get_cryptocurrency_usd_rate(quote_asset, timestamp).await?;
            with_cache_mut(|mut cache| {
                cache.insert(quote_rate.clone(), time);
            });
            quote_rate
        }
    };

    Ok(base_rate / quote_rate)
}

#[allow(unused_variables)]
async fn handle_crypto_base_fiat_quote_pair(
    caller: &Principal,
    base_asset: &Asset,
    quote_asset: &Asset,
    timestamp: u64,
) -> Result<QueriedExchangeRate, ExchangeRateError> {
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

async fn get_cryptocurrency_usd_rate(
    asset: &Asset,
    timestamp: u64,
) -> Result<QueriedExchangeRate, ExchangeRateError> {
    let results = join_all(EXCHANGES.iter().map(|exchange| {
        call_exchange(
            exchange,
            CallExchangesArgs {
                timestamp,
                quote_asset: exchange.supported_usd_asset_type(),
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

    // TODO: Convert the rates to USD.
    let num_queried_sources = rates.len();

    Ok(QueriedExchangeRate {
        base_asset: asset.clone(),
        quote_asset: Asset {
            symbol: "USD".to_string(),
            class: AssetClass::FiatCurrency,
        },
        timestamp,
        rates,
        num_queried_sources,
        num_received_rates: num_queried_sources + errors.len(),
    })
}
