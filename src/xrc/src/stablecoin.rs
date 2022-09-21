use crate::candid::{Asset, ExchangeRate, ExchangeRateMetadata};

/// At least 2 stablecoin rates with respect to a third stablecoin are needed to determine if a rate is off.
pub(crate) const MIN_NUM_STABLECOIN_RATES: usize = 2;

/// Represents the errors when attempting to extract a value from JSON.
#[derive(Debug)]
pub(crate) enum StablecoinRateError {
    TooFewRates(usize),
    DifferentQuoteAssets(Asset, Asset),
    ZeroRate,
}

// The function computes the scaled (permyriad) standard deviation of the
// given rates.
fn standard_deviation_permyriad(rates: &[u64]) -> u64 {
    let sum: u64 = rates.iter().sum();
    let count = rates.len() as u64;
    let mean: i64 = (sum / count) as i64;
    let variance = rates
        .iter()
        .map(|rate| (((*rate as i64) - mean).pow(2)) as u64)
        .sum::<u64>()
        / count;
    // Note that the variance has a scaling factor of 10_000^2.
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
    target: &Asset,
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
        .find(|rate| &rate.quote_asset != quote_asset)
    {
        return Err(StablecoinRateError::DifferentQuoteAssets(
            quote_asset.clone(),
            rate.quote_asset.clone(),
        ));
    }

    // Extract the median rate.
    let mut rates: Vec<_> = stablecoin_rates
        .iter()
        .map(|rate| rate.rate_permyriad)
        .collect();
    rates.sort();
    let length = stablecoin_rates.len();

    let median_rate = if length % 2 == 0 {
        (rates[(length / 2) - 1] + rates[length / 2]) / 2
    } else {
        rates[length / 2]
    };

    if median_rate == 0 {
        return Err(StablecoinRateError::ZeroRate);
    }

    // Turn the S/Q rate into the Q/S = Q/T rate (permyriad).
    let target_rate = 100_000_000 / median_rate;

    let standard_deviation = standard_deviation_permyriad(&rates);

    // The returned exchange rate uses the median timestamp.
    let mut timestamps: Vec<_> = stablecoin_rates.iter().map(|rate| rate.timestamp).collect();
    timestamps.sort();
    let median_timestamp = timestamps[length / 2];

    Ok(ExchangeRate {
        base_asset: Asset {
            symbol: quote_asset.symbol.clone(),
            class: quote_asset.class.clone(),
        },
        quote_asset: target.clone(),
        timestamp: median_timestamp,
        rate_permyriad: target_rate,
        metadata: ExchangeRateMetadata {
            number_of_queried_sources: stablecoin_rates.len(),
            number_of_received_rates: stablecoin_rates.len(),
            standard_deviation_permyriad: standard_deviation,
        },
    })
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::candid::AssetClass;
    use rand::seq::SliceRandom;
    use rand::Rng;

    fn generate_stablecoin_rates(num_rates: usize, median_rate: u64) -> Vec<ExchangeRate> {
        let mut rates = vec![];
        let mut rates_permyriad = vec![median_rate; num_rates];
        // Change less than half of the rates arbitrarily.
        let num_changed = if num_rates % 2 == 0 {
            (num_rates - 1) / 2
        } else {
            num_rates / 2
        };

        let mut rng = rand::thread_rng();
        let range: i64 = (median_rate / 10) as i64;

        for rate in rates_permyriad.iter_mut().take(num_changed) {
            let change: i64 = rng.gen_range(0..2 * range) - range;
            *rate = ((*rate as i64) + change) as u64;
        }
        rates_permyriad.shuffle(&mut rng);

        for (index, rate) in rates_permyriad.iter().enumerate() {
            let rate = ExchangeRate {
                base_asset: Asset {
                    symbol: ["BA_", &index.to_string()].join(""),
                    class: AssetClass::Cryptocurrency,
                },
                quote_asset: Asset {
                    symbol: "QA".to_string(),
                    class: AssetClass::Cryptocurrency,
                },
                timestamp: 1647734400,
                rate_permyriad: *rate,
                metadata: ExchangeRateMetadata {
                    number_of_queried_sources: 0,
                    number_of_received_rates: 0,
                    standard_deviation_permyriad: 0,
                },
            };
            rates.push(rate);
        }
        rates
    }

    /// The function tests that the appropriate error is returned when fewer than
    /// [MIN_NUM_STABLECOIN_RATES] rates are provided.
    #[test]
    fn stablecoin_test_not_enough_rates() {
        let rates = generate_stablecoin_rates(1, 10_000);
        let target = Asset {
            symbol: "TA".to_string(),
            class: AssetClass::FiatCurrency,
        };

        let stablecoin_rate = get_stablecoin_rate(&rates, &target);

        assert!(matches!(
            stablecoin_rate,
            Err(StablecoinRateError::TooFewRates(1))
        ));
    }

    /// The function tests that the appropriate error is returned when there is a mismatch between
    /// quote assets.
    #[test]
    fn stablecoin_test_different_quote_assets() {
        let mut rates = generate_stablecoin_rates(2, 10_000);
        rates[0].quote_asset.symbol = "DA".to_string();
        let target = Asset {
            symbol: "TA".to_string(),
            class: AssetClass::FiatCurrency,
        };

        let stablecoin_rate = get_stablecoin_rate(&rates, &target);

        assert!(matches!(
            stablecoin_rate,
            Err(StablecoinRateError::DifferentQuoteAssets(_, _))
        ));
    }

    /// The function tests that the appropriate error is returned when there is a rate of zero.
    #[test]
    fn stablecoin_test_zero_rate() {
        let rates = generate_stablecoin_rates(2, 0);
        let target = Asset {
            symbol: "TA".to_string(),
            class: AssetClass::FiatCurrency,
        };

        let stablecoin_rate = get_stablecoin_rate(&rates, &target);

        assert!(matches!(
            stablecoin_rate,
            Err(StablecoinRateError::ZeroRate)
        ));
    }

    /// The function tests that the correct rate is returned if the majority of rates
    /// are pegged to the target currency for the case that the quote asset is also pegged.
    #[test]
    fn stablecoin_test_with_pegged_quote_asset() {
        let mut rng = rand::thread_rng();
        let num_rates = rng.gen_range(2..10);
        let rates = generate_stablecoin_rates(num_rates, 10_000);
        let target = Asset {
            symbol: "TA".to_string(),
            class: AssetClass::FiatCurrency,
        };

        let stablecoin_rate = get_stablecoin_rate(&rates, &target);

        let rates_permyriad: Vec<_> = rates.iter().map(|rate| rate.rate_permyriad).collect();
        let standard_deviation = standard_deviation_permyriad(&rates_permyriad);

        let expected_rate = ExchangeRate {
            base_asset: rates[0].quote_asset.clone(),
            quote_asset: target,
            timestamp: 1647734400,
            rate_permyriad: 10_000,
            metadata: ExchangeRateMetadata {
                number_of_queried_sources: num_rates,
                number_of_received_rates: num_rates,
                standard_deviation_permyriad: standard_deviation,
            },
        };
        assert!(matches!(stablecoin_rate, Ok(rate) if rate == expected_rate));
    }

    /// The function tests that the correct rate is returned if the majority of rates
    /// are pegged to the target currency for the case that the quote asset got depegged.
    #[test]
    fn stablecoin_test_with_depegged_quote_asset() {
        let mut rng = rand::thread_rng();
        let num_rates = rng.gen_range(2..10);
        let difference = rng.gen_range(0..19000) - 8500;
        let median_rate = (10_000 + difference) as u64;

        let rates = generate_stablecoin_rates(num_rates, median_rate);
        let target = Asset {
            symbol: "TA".to_string(),
            class: AssetClass::FiatCurrency,
        };

        let stablecoin_rate = get_stablecoin_rate(&rates, &target);
        // The expected rate is the inverse of the median rate.
        let expected_rate = 100_000_000 / median_rate;
        assert!(matches!(stablecoin_rate, Ok(rate) if rate.rate_permyriad == expected_rate));
    }
}
