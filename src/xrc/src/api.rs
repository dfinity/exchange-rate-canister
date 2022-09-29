use crate::{
    call_exchange,
    candid::{
        Asset, AssetClass, ExchangeRate, ExchangeRateError, ExchangeRateMetadata,
        GetExchangeRateRequest, GetExchangeRateResult,
    },
    utils, CallExchangesArgs, EXCHANGES,
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
    match (&request.base_asset.class, &request.quote_asset.class) {
        (AssetClass::Cryptocurrency, AssetClass::Cryptocurrency) => {
            handle_cryptocurrency_pair(&caller, &request, timestamp).await
        }
        (AssetClass::Cryptocurrency, AssetClass::FiatCurrency) => todo!(),
        (AssetClass::FiatCurrency, AssetClass::Cryptocurrency) => todo!(),
        (AssetClass::FiatCurrency, AssetClass::FiatCurrency) => todo!(),
    }
}

async fn handle_cryptocurrency_pair(
    caller: &Principal,
    request: &GetExchangeRateRequest,
    timestamp: u64,
) -> GetExchangeRateResult {
    // TODO: Check if items are in the cache here.
    // TODO: Check if stablecoins are in the cache here.

    if !utils::is_caller_the_cmc(caller) && !has_capacity() {
        // TODO: replace with variant errors for better clarity
        return Err(ExchangeRateError {
            code: 0,
            description: "Rate limited".to_string(),
        });
    }

    let base_rate = get_cryptocurrency_usd_rate(&request.base_asset, timestamp).await?;
    let quote_rate = get_cryptocurrency_usd_rate(&request.quote_asset, timestamp).await?;
    // TODO: get missing stablecoin rates
    //stablecoin::get_stablecoin_rate(stablecoin_rates, target);
    Ok(base_rate / quote_rate)
}

// TODO: replace this function with an actual implementation
fn has_capacity() -> bool {
    true
}

async fn get_cryptocurrency_usd_rate(asset: &Asset, timestamp: u64) -> GetExchangeRateResult {
<<<<<<< HEAD
    // Otherwise, retrieve the asset USD rate.
=======
>>>>>>> b063e7dca32d55bcb8a369f6556b4a0b34b142f9
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

    let rate = ExchangeRate {
        base_asset: asset.clone(),
        quote_asset: Asset {
            symbol: "USD".to_string(),
            class: AssetClass::FiatCurrency,
        },
        timestamp,
        rate_permyriad: utils::get_median(&mut rates),
        metadata: ExchangeRateMetadata {
            number_of_queried_sources: rates.len() + errors.len(),
            number_of_received_rates: rates.len(),
            standard_deviation_permyriad: 0,
        },
    };

    Ok(rate)
}
