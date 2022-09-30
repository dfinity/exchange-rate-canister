use crate::{
    call_exchange,
    candid::{Asset, AssetClass, ExchangeRateError, GetExchangeRateRequest, GetExchangeRateResult},
    utils, CallExchangesArgs, QueriedExchangeRate, EXCHANGES,
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
            handle_mixed_pair(
                &caller,
                &request.base_asset,
                &request.quote_asset,
                timestamp,
            )
            .await
        }
        #[rustfmt::skip]
        (AssetClass::FiatCurrency, AssetClass::Cryptocurrency) => {
            handle_mixed_pair(
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

    result.map(|r| r.into())
}

async fn handle_cryptocurrency_pair(
    caller: &Principal,
    base_asset: &Asset,
    quote_asset: &Asset,
    timestamp: u64,
) -> Result<QueriedExchangeRate, ExchangeRateError> {
    // TODO: Check if items are in the cache here.
    // TODO: Check if stablecoins are in the cache here.

    if !utils::is_caller_the_cmc(caller) && !has_capacity() {
        // TODO: replace with variant errors for better clarity
        return Err(ExchangeRateError {
            code: 0,
            description: "Rate limited".to_string(),
        });
    }

    let base_rate = get_cryptocurrency_usd_rate(base_asset, timestamp).await?;
    let quote_rate = get_cryptocurrency_usd_rate(quote_asset, timestamp).await?;
    // TODO: get missing stablecoin rates
    //stablecoin::get_stablecoin_rate(stablecoin_rates, target);
    Ok(base_rate / quote_rate)
}

#[allow(unused_variables)]
async fn handle_mixed_pair(
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
