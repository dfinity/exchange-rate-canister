use crate::candid::{Asset, AssetClass, ExchangeRate, ExchangeRateMetadata};

/// At least 2 stablecoin rates are needed to determine if a rate is off.
pub(crate) const MIN_NUM_STABLECOIN_RATES: usize = 2;

/// Constant timestamp for the stablecoin rate.
pub(crate) const STABLECOIN_RATE_TIMESTAMP: u64 = 0;

/// Represents the errors when attempting to extract a value from JSON.
#[derive(Debug)]
pub(crate) enum StablecoinRateError {
    TooFewRates(usize),
    DifferentQuoteAssets(Asset, Asset),
    ZeroRate(ExchangeRate),
}

// The function computes the scaled (permyriad) standard deviation of the
// given rates.
fn standard_deviation_permyriad(rates: &[u64]) -> u64 {
    let sum: u64 = rates.iter().sum();
    let count = rates.len() as u64;
    let mean = sum / count;
    let variance = rates.iter().map(|rate| (rate - mean).pow(2)).sum::<u64>() / count;
    // Note that the variance has scaling factor of 10_000^2.
    // The square root reduces the scaling factor back to 10_000.
    (variance as f64).sqrt() as u64
}
/// Given a set of stablecoin exchange rates all pegged to the same target fiat currency T
/// and with the same quote asset Q but different base assets, the function determines the
/// stablecoin S that is most consistent with the other stablecoins and is therefore the best
/// approximation for the target fiat currency T and returns Q/S as an estimate for Q/T.
#[allow(dead_code)]
pub(crate) fn get_stablecoin_rate(
    stablecoin_rates: &[ExchangeRate],
    target_symbol: &str,
) -> Result<ExchangeRate, StablecoinRateError> {
    if stablecoin_rates.len() < MIN_NUM_STABLECOIN_RATES {
        return Err(StablecoinRateError::TooFewRates(stablecoin_rates.len()));
    }
    let quote_asset = &stablecoin_rates
        .get(0)
        .expect("There should always be at least one rate")
        .quote_asset;

    if let Some(rate) = stablecoin_rates
        .iter()
        .find(|rate| rate.quote_asset.symbol.to_uppercase() != quote_asset.symbol)
    {
        return Err(StablecoinRateError::DifferentQuoteAssets(
            quote_asset.clone(),
            rate.quote_asset.clone(),
        ));
    }
    if let Some(rate) = stablecoin_rates
        .iter()
        .find(|rate| rate.rate_permyriad == 0)
    {
        return Err(StablecoinRateError::ZeroRate(rate.clone()));
    }

    // Collect the rates and determine the stablecoin that is most in line with the other stablecoins.
    let mut rates: Vec<_> = stablecoin_rates
        .iter()
        .map(|rate| rate.rate_permyriad)
        .collect();
    // Add the quote asset/quote asset pair itself so that all stablecoins can be mutually compared.
    rates.push(10_000);
    let mut min_error = u64::MAX;
    let mut min_error_rate = u64::MAX;
    let mut min_standard_deviation = u64::MAX;
    for current_rate in &rates {
        let mut adapted_rates: Vec<_> = rates
            .iter()
            .map(|rate| (rate * 10_000) / current_rate)
            .collect();
        adapted_rates.sort();
        let length = adapted_rates.len();
        let median_rate = if length % 2 == 0 {
            (adapted_rates[(length / 2) - 1] + adapted_rates[length / 2]) / 2
        } else {
            adapted_rates[length]
        };
        let error = i64::abs_diff(10_000, median_rate as i64);
        if error < min_error {
            min_error = error;
            // The last vector entry constitutes the current best stablecoin rate.
            min_error_rate = adapted_rates[length - 1];
            min_standard_deviation = standard_deviation_permyriad(&adapted_rates);
        }
    }
    Ok(ExchangeRate {
        base_asset: Asset {
            symbol: quote_asset.symbol.clone(),
            class: AssetClass::Cryptocurrency,
        },
        quote_asset: Asset {
            symbol: target_symbol.to_string(),
            class: AssetClass::FiatCurrency,
        },
        timestamp: STABLECOIN_RATE_TIMESTAMP,
        rate_permyriad: min_error_rate,
        metadata: ExchangeRateMetadata {
            number_of_queried_sources: stablecoin_rates.len() as u64,
            number_of_received_rates: stablecoin_rates.len() as u64,
            standard_deviation_permyriad: min_standard_deviation,
        },
    })
}

#[cfg(test)]
mod test {}
