use crate::{
    call_exchange,
    candid::{
        Asset, AssetClass, ExchangeRate, ExchangeRateError, ExchangeRateMetadata,
        GetExchangeRateRequest, GetExchangeRateResult,
    },
    utils, with_cache_mut, CallExchangesArgs, EXCHANGES,
};
use futures::future::join_all;
use ic_cdk::export::Principal;

/// Id of the cycles minting canister on the IC.
pub const MAINNET_CYCLES_MINTING_CANISTER_ID: Principal =
    Principal::from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x01, 0x01]);

/// This function routes the request to the appropriate handler function by
pub async fn get_exchange_rate(
    caller: Principal,
    request: GetExchangeRateRequest,
) -> GetExchangeRateResult {
    if !is_caller_the_cmc(&caller) && !has_capacity() {
        // TODO: replace with variant errors for better clarity
        return Err(ExchangeRateError {
            code: 0,
            description: "Rate limited".to_string(),
        });
    }

    let timestamp = get_normalized_timestamp(&request);

    // Route the call based on the provided asset types.
    get_rate(request.base_asset, request.quote_asset, timestamp).await
}

fn get_normalized_timestamp(request: &GetExchangeRateRequest) -> u64 {
    (request.timestamp.unwrap_or_else(utils::time_secs) / 60) * 60
}

fn is_caller_the_cmc(caller: &Principal) -> bool {
    *caller == MAINNET_CYCLES_MINTING_CANISTER_ID
}

// TODO: replace this function with an actual implementation
fn has_capacity() -> bool {
    true
}

async fn get_rate(base_asset: Asset, quote_asset: Asset, timestamp: u64) -> GetExchangeRateResult {
    match (&base_asset.class, &quote_asset.class) {
        (AssetClass::Cryptocurrency, AssetClass::Cryptocurrency) => {
            let base_rate = get_cryptocurrency_usd_rate(&base_asset, timestamp).await?;
            let quote_rate = get_cryptocurrency_usd_rate(&quote_asset, timestamp).await?;
            // Temporary...
            Ok(base_rate)
        }
        (AssetClass::Cryptocurrency, AssetClass::FiatCurrency) => todo!(),
        (AssetClass::FiatCurrency, AssetClass::Cryptocurrency) => todo!(),
        (AssetClass::FiatCurrency, AssetClass::FiatCurrency) => todo!(),
    }
}

async fn get_cryptocurrency_usd_rate(asset: &Asset, timestamp: u64) -> GetExchangeRateResult {
    // TODO: Attempt to get rate from cache. If in cache, return rate.
    let current_time = utils::time_secs();
    let maybe_rate = with_cache_mut(|cache| cache.get(&asset.symbol, timestamp, current_time));
    if let Some(rate) = maybe_rate {
        ic_cdk::println!("Retrieved rate through the cache!");
        return Ok(rate);
    }

    // Otherwise, retrieve the asset USD rate.
    let results = join_all(EXCHANGES.iter().map(|exchange| {
        call_exchange(
            exchange,
            CallExchangesArgs {
                timestamp,
                quote_asset: Asset {
                    symbol: "USD".to_string(),
                    class: AssetClass::FiatCurrency,
                },
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

    ic_cdk::println!("=== Rates & Errors ===");
    ic_cdk::println!("{:#?}", rates);
    ic_cdk::println!("{:#?}", errors);
    ic_cdk::println!("======================");

    // Handle error case here where rates could be empty from total failure.

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
    let rate_clone = rate.clone();

    with_cache_mut(|cache| {
        cache.insert(rate_clone, current_time);
    });

    Ok(rate)
}
