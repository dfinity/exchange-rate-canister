use crate::candid::Asset;
use crate::utils::median;
use crate::QueriedExchangeRate;

/// At least 2 stablecoin rates with respect to a third stablecoin are needed to determine if a rate is off.
pub(crate) const MIN_NUM_STABLECOIN_RATES: usize = 2;

/// Represents the errors when attempting to extract a value from JSON.
#[derive(Debug)]
pub(crate) enum StablecoinRateError {
    TooFewRates(usize),
    DifferentQuoteAssets(Asset, Asset),
    ZeroRate,
}

/// Given a set of stablecoin exchange rates all pegged to the same target fiat currency T
/// and with the same quote asset Q but different base assets, the function determines the
/// stablecoin S that is most consistent with the other stablecoins and is therefore the best
/// approximation for the target fiat currency T and returns Q/S as an estimate for Q/T.
#[allow(dead_code)]
pub(crate) fn get_stablecoin_rate(
    stablecoin_rates: &[QueriedExchangeRate],
    target: &Asset,
) -> Result<QueriedExchangeRate, StablecoinRateError> {
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

    let indexed_median_rates: Vec<_> = stablecoin_rates
        .iter()
        .enumerate()
        .map(|(index, rate)| (index, median(&rate.rates)))
        .collect();

    let median_rates: Vec<_> = indexed_median_rates
        .iter()
        .map(|(_, median)| *median)
        .collect();
    let median_of_median = median(&median_rates);

    if median_of_median == 0 {
        return Err(StablecoinRateError::ZeroRate);
    }

    // Retrieve the corresponding index.
    let (median_index, _) = indexed_median_rates
        .iter()
        .find(|(_, median)| *median == median_of_median)
        .expect("The stablecoin median rate must be found.");

    let median_stablecoin_rate = stablecoin_rates
        .get(*median_index)
        .expect("The stablecoin exchange rate must exist.");

    // The returned exchange rate uses the median timestamp.
    let timestamps: Vec<_> = stablecoin_rates.iter().map(|rate| rate.timestamp).collect();
    // The exchange rate canister uses timestamps without seconds.
    let median_timestamp = (median(&timestamps) / 60) * 60;

    // Construct the S/Q exchange rate struct.
    let target_to_quote_rate = QueriedExchangeRate {
        base_asset: target.clone(),
        quote_asset: Asset {
            symbol: quote_asset.symbol.clone(),
            class: quote_asset.class.clone(),
        },
        timestamp: median_timestamp,
        rates: median_stablecoin_rate.rates.clone(),
        num_queried_sources: median_stablecoin_rate.num_queried_sources,
        num_received_rates: median_stablecoin_rate.num_received_rates,
    };

    // Turn the S/Q rate into the Q/S = Q/T rate.
    Ok(target_to_quote_rate.inverted())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::candid::AssetClass;
    use rand::seq::SliceRandom;
    use rand::Rng;

    fn generate_stablecoin_rates(num_rates: usize, median_rate: u64) -> Vec<QueriedExchangeRate> {
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
            let rate = QueriedExchangeRate {
                base_asset: Asset {
                    symbol: ["BA_", &index.to_string()].join(""),
                    class: AssetClass::Cryptocurrency,
                },
                quote_asset: Asset {
                    symbol: "QA".to_string(),
                    class: AssetClass::Cryptocurrency,
                },
                timestamp: 1647734400,
                rates: vec![*rate],
                num_queried_sources: 1,
                num_received_rates: 1,
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

        let expected_rate = QueriedExchangeRate {
            base_asset: rates[0].quote_asset.clone(),
            quote_asset: target,
            timestamp: 1647734400,
            rates: vec![10_000],
            num_queried_sources: 1,
            num_received_rates: 1,
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
        assert!(matches!(stablecoin_rate, Ok(rate) if rate.rates[0] == expected_rate));
    }
}
