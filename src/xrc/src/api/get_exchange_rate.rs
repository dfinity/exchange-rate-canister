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

fn has_capacity() -> bool {
    true
}

async fn get_rate(base_asset: Asset, quote_asset: Asset, timestamp: u64) -> GetExchangeRateResult {
    match (&base_asset.class, &quote_asset.class) {
        (AssetClass::Cryptocurrency, AssetClass::Cryptocurrency) => {
            handle_cryptocurrency_only(&base_asset, &quote_asset, timestamp).await
        }
        (AssetClass::Cryptocurrency, AssetClass::FiatCurrency) => todo!(),
        (AssetClass::FiatCurrency, AssetClass::Cryptocurrency) => todo!(),
        (AssetClass::FiatCurrency, AssetClass::FiatCurrency) => todo!(),
    }
}

async fn handle_cryptocurrency_only(
    base_asset: &Asset,
    quote_asset: &Asset,
    timestamp: u64,
) -> GetExchangeRateResult {
    let base_rate = get_cryptocurrency_usd_rate(&base_asset, timestamp).await?;
    let quote_rate = get_cryptocurrency_usd_rate(&quote_asset, timestamp).await?;
    Ok(ExchangeRate {
        base_asset: base_rate.base_asset,
        quote_asset: quote_rate.base_asset,
        timestamp: base_rate.timestamp,
        rate_permyriad: base_rate.rate_permyriad / quote_rate.rate_permyriad,
        metadata: ExchangeRateMetadata {
            number_of_queried_sources: base_rate.metadata.number_of_queried_sources
                + quote_rate.metadata.number_of_queried_sources,
            number_of_received_rates: base_rate.metadata.number_of_received_rates
                + quote_rate.metadata.number_of_received_rates,
            standard_deviation_permyriad: 0,
        },
    })
}

async fn get_cryptocurrency_usd_rate(asset: &Asset, timestamp: u64) -> GetExchangeRateResult {
    // TODO: Attempt to get rate from cache. If in cache, return rate.

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
    // TODO: cache the rate here

    Ok(rate)
}
