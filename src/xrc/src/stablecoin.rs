use ic_xrc_types::{Asset, ExchangeRateError};

use crate::utils::{median, median_in_set};
use crate::QueriedExchangeRate;

/// At least 2 stablecoin rates - each quoted against the quote asset USDT (the
/// "third stablecoin") - are needed to determine if a rate is off. USDT is the
/// denominator, not a candidate in the median; see `get_stablecoin_rate`.
pub(crate) const MIN_NUM_STABLECOIN_RATES: usize = 2;

/// Represents the errors when attempting to extract a value from JSON.
#[derive(Debug)]
pub(crate) enum StablecoinRateError {
    TooFewRates(usize),
    DifferentQuoteAssets(Asset, Asset),
    ZeroRate,
}

impl From<StablecoinRateError> for ExchangeRateError {
    fn from(error: StablecoinRateError) -> Self {
        match error {
            StablecoinRateError::TooFewRates(_) => ExchangeRateError::StablecoinRateTooFewRates,
            StablecoinRateError::DifferentQuoteAssets(_, _) => {
                ExchangeRateError::StablecoinRateNotFound
            }
            StablecoinRateError::ZeroRate => ExchangeRateError::StablecoinRateZeroRate,
        }
    }
}

impl core::fmt::Display for StablecoinRateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StablecoinRateError::TooFewRates(num_rates) => write!(
                f,
                "Not enough stablecoin rates were provided (found {})",
                num_rates
            ),
            StablecoinRateError::DifferentQuoteAssets(expected_asset, quote_asset) => write!(
                f,
                "Stablecoins provided have different quote assets (expected: {}, found: {}) ",
                expected_asset.symbol, quote_asset.symbol
            ),
            StablecoinRateError::ZeroRate => write!(f, "Calculated stablecoin rate is zero"),
        }
    }
}

/// Given a set of stablecoin exchange rates all pegged to the same target fiat currency T
/// and with the same quote asset Q but different base assets, the function determines the
/// stablecoin S that is most consistent with the other stablecoins and is therefore the best
/// approximation for the target fiat currency T and returns Q/S as an estimate for Q/T.
pub(crate) fn get_stablecoin_rate(
    stablecoin_rates: &[QueriedExchangeRate],
    target: &Asset,
) -> Result<QueriedExchangeRate, StablecoinRateError> {
    if stablecoin_rates.len() < MIN_NUM_STABLECOIN_RATES {
        return Err(StablecoinRateError::TooFewRates(stablecoin_rates.len()));
    }
    let quote_asset = &stablecoin_rates
        .first()
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
    // The median must exist in the set of rates.
    let median_of_median = median_in_set(&median_rates);

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
    let quote_asset = Asset {
        symbol: quote_asset.symbol.clone(),
        class: quote_asset.class.clone(),
    };
    let target_to_quote_rate = QueriedExchangeRate::new(
        target.clone(),
        quote_asset,
        median_timestamp,
        &median_stablecoin_rate.rates,
        median_stablecoin_rate.base_asset_num_queried_sources,
        median_stablecoin_rate.base_asset_num_received_rates,
        None,
    );

    // Turn the S/Q rate into the Q/S = Q/T rate.
    Ok(target_to_quote_rate.inverted())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{utils, DECIMALS, RATE_UNIT};
    use ic_xrc_types::AssetClass;
    use rand::seq::SliceRandom;
    use rand::Rng;

    fn generate_stablecoin_rates(num_rates: usize, median_rate: u64) -> Vec<QueriedExchangeRate> {
        let mut rates = vec![];
        let mut initial_rates = vec![median_rate; num_rates];
        // Change less than half of the rates arbitrarily.
        let num_changed = if num_rates.is_multiple_of(2) {
            (num_rates - 1) / 2
        } else {
            num_rates / 2
        };

        let mut rng = rand::rng();
        let range: i64 = (median_rate / 10) as i64;

        for rate in initial_rates.iter_mut().take(num_changed) {
            let change: i64 = rng.random_range(0..2 * range) - range;
            *rate = ((*rate as i64) + change) as u64;
        }
        initial_rates.shuffle(&mut rng);

        for (index, rate) in initial_rates.iter().enumerate() {
            let rate = QueriedExchangeRate::new(
                Asset {
                    symbol: ["BA", &index.to_string()].join(""),
                    class: AssetClass::Cryptocurrency,
                },
                Asset {
                    symbol: "QA".to_string(),
                    class: AssetClass::Cryptocurrency,
                },
                1647734400,
                &[*rate],
                1,
                1,
                None,
            );
            rates.push(rate);
        }
        rates
    }

    /// The function tests that the appropriate error is returned when fewer than
    /// [MIN_NUM_STABLECOIN_RATES] rates are provided.
    #[test]
    fn stablecoin_not_enough_rates() {
        let rates = generate_stablecoin_rates(1, RATE_UNIT);
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
    fn stablecoin_different_quote_assets() {
        let mut rates = generate_stablecoin_rates(2, RATE_UNIT);
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
    fn stablecoin_zero_rate() {
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
    fn stablecoin_pegged_quote_asset() {
        let mut rng = rand::rng();
        let num_rates = rng.random_range(2..10);
        let rates = generate_stablecoin_rates(num_rates, RATE_UNIT);
        let target = Asset {
            symbol: "TA".to_string(),
            class: AssetClass::FiatCurrency,
        };

        let stablecoin_rate = get_stablecoin_rate(&rates, &target);

        let expected_rate = QueriedExchangeRate::new(
            rates[0].quote_asset.clone(),
            target,
            1647734400,
            &[RATE_UNIT],
            1,
            1,
            None,
        );
        assert!(matches!(stablecoin_rate, Ok(rate) if rate == expected_rate));
    }

    /// The function tests that the correct rate is returned if the majority of rates
    /// are pegged to the target currency for the case that the quote asset got depegged.
    #[test]
    fn stablecoin_depegged_quote_asset() {
        let mut rng = rand::rng();
        let num_rates = rng.random_range(2..10);
        let difference = (rng.random_range(0..19000) as u64).saturating_sub(8500);
        let median_rate = RATE_UNIT + difference;

        let rates = generate_stablecoin_rates(num_rates, median_rate);
        let target = Asset {
            symbol: "TA".to_string(),
            class: AssetClass::FiatCurrency,
        };

        let stablecoin_rate = get_stablecoin_rate(&rates, &target);
        // The expected rate is the inverse of the median rate.
        let expected_rate = utils::checked_invert_rate(median_rate.into(), DECIMALS)
            .expect("should be able to invert the rate");
        assert!(matches!(stablecoin_rate, Ok(rate) if rate.rates[0] == expected_rate));
    }

    /// The function tests that the stablecoin with the median rate is returned.
    /// Specifically, the three stablecoins in the test have the following median rates:
    ///
    /// - median(11001, 10998, 11055, 10909) = 10999
    /// - median(9919, 9814, 10008) = 9919
    /// - median(9991, 10312, 10123, 9614, 11123) = 10123
    ///
    /// The third stablecoin has the median-of-median rate and is used as the rate of the target asset.
    #[test]
    fn stablecoin_median_of_median() {
        let first_rate = QueriedExchangeRate::new(
            Asset {
                symbol: "A".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            Asset {
                symbol: "B".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            0,
            &[11001, 10998, 11055, 10909],
            4,
            4,
            None,
        );
        let second_rate = QueriedExchangeRate::new(
            Asset {
                symbol: "C".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            Asset {
                symbol: "B".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            0,
            &[9919, 9814, 10008],
            3,
            3,
            None,
        );
        let third_rate = QueriedExchangeRate::new(
            Asset {
                symbol: "D".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            Asset {
                symbol: "B".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            0,
            &[9991, 10312, 10123, 9614, 11123],
            5,
            5,
            None,
        );
        let target_asset = Asset {
            symbol: "T".to_string(),
            class: AssetClass::FiatCurrency,
        };
        let computed_rate =
            get_stablecoin_rate(&[first_rate, second_rate, third_rate], &target_asset);
        let expected_rate = QueriedExchangeRate::new(
            Asset {
                symbol: "T".to_string(),
                class: AssetClass::FiatCurrency,
            },
            Asset {
                symbol: "B".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            0,
            &[9991, 10312, 10123, 9614, 11123],
            5,
            5,
            None,
        )
        .inverted();
        assert!(matches!(computed_rate, Ok(rate) if rate == expected_rate));
    }

    /// The function tests that a stablecoin rate is computed successfully
    /// if the number of rates is even.
    /// Specifically, the four stablecoins in the test have the following median rates:
    ///
    /// - median(11001, 10998, 11055, 10909) = 10999
    /// - median(9919, 9814, 10008) = 9919
    /// - median(9991, 10312, 10123, 9614, 11123) = 10123
    /// - median(9988, 10101) = 10044
    ///
    /// The third stablecoin has the median-of-median rate and is used as the rate of the target asset.
    #[test]
    fn stablecoin_even_number_of_rates() {
        let first_rate = QueriedExchangeRate::new(
            Asset {
                symbol: "A".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            Asset {
                symbol: "B".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            0,
            &[11001, 10998, 11055, 10909],
            4,
            4,
            None,
        );
        let second_rate = QueriedExchangeRate::new(
            Asset {
                symbol: "C".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            Asset {
                symbol: "B".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            0,
            &[9919, 9814, 10008],
            3,
            3,
            None,
        );
        let third_rate = QueriedExchangeRate::new(
            Asset {
                symbol: "D".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            Asset {
                symbol: "B".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            0,
            &[9991, 10312, 10123, 9614, 11123],
            5,
            5,
            None,
        );
        let fourth_rate = QueriedExchangeRate::new(
            Asset {
                symbol: "E".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            Asset {
                symbol: "B".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            0,
            &[9988, 10101],
            2,
            2,
            None,
        );
        let target_asset = Asset {
            symbol: "T".to_string(),
            class: AssetClass::FiatCurrency,
        };
        // The true median is 10083 and the fourth rate has the closest median at 10044,
        // so this rate is returned.
        let computed_rate = get_stablecoin_rate(
            &[first_rate, second_rate, third_rate, fourth_rate],
            &target_asset,
        );
        let expected_rate = QueriedExchangeRate::new(
            Asset {
                symbol: "T".to_string(),
                class: AssetClass::FiatCurrency,
            },
            Asset {
                symbol: "B".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            0,
            &[9988, 10101],
            2,
            2,
            None,
        )
        .inverted();
        assert!(matches!(computed_rate, Ok(rate) if rate == expected_rate));
    }

    /// Documents the ACTUAL selection behaviour for the two stablecoin symbols
    /// the canister uses in production (`STABLECOIN_BASES = [USDS, USDC]`).
    ///
    /// Design slides describe a three-input median over `{median_usdc,
    /// median_dai, median_usdt = 1}` that rejects a single depegged
    /// stablecoin. The implementation does NOT match that: it takes
    /// `median_in_set` over only the two real stablecoin medians and injects
    /// no synthetic `USDT = 1` anchor. With exactly two inputs there is no true
    /// "middle", so a depegged stablecoin is not rejected — it is selected and
    /// flows straight into the returned USDT/USD rate.
    ///
    /// Using the slides' "DAI depegged" example (USDS/USDT median 0.80,
    /// USDC/USDT median 0.99), the code returns USDT/USD = 1 / 0.80 = 1.25,
    /// NOT the slides' 1 / 0.99 = 1.01.
    #[test]
    fn two_symbol_set_does_not_reject_a_depegged_stablecoin() {
        let usdt = Asset {
            symbol: "USDT".to_string(),
            class: AssetClass::Cryptocurrency,
        };
        // Order mirrors STABLECOIN_BASES = [USDS, USDC].
        let depegged_usds = QueriedExchangeRate::new(
            Asset {
                symbol: "USDS".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            usdt.clone(),
            1647734400,
            &[800_000_000], // USDS/USDT = 0.80 (depegged)
            1,
            1,
            None,
        );
        let healthy_usdc = QueriedExchangeRate::new(
            Asset {
                symbol: "USDC".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            usdt,
            1647734400,
            &[990_000_000], // USDC/USDT = 0.99
            1,
            1,
            None,
        );

        let stablecoin_rate =
            get_stablecoin_rate(&[depegged_usds, healthy_usdc], &crate::api::usd_asset())
                .expect("a stablecoin rate should be returned");

        // The result is USDT/USD: the selected stablecoin is treated as USD and inverted.
        assert_eq!(stablecoin_rate.base_asset.symbol, "USDT");
        assert_eq!(stablecoin_rate.quote_asset.symbol, "USD");

        // The depegged USDS (0.80) was selected: 1 / 0.80 = 1.25 (RATE_UNIT scaled).
        // It was NOT rejected in favour of the healthy USDC (1 / 0.99 = 1.01),
        // confirming there is no median-of-three and no synthetic USDT = 1 anchor.
        assert_eq!(stablecoin_rate.rates, vec![1_250_000_000]);
        assert_ne!(stablecoin_rate.rates, vec![1_010_101_010]);
    }

    /// Documents that with exactly TWO stablecoin symbols the selection is
    /// ORDER-DEPENDENT. `median_in_set` has no true middle for two values: they
    /// are equidistant from their midpoint, so the tie is broken toward the
    /// FIRST entry (it only replaces on a strictly-smaller distance). Because
    /// real rates are even-valued in RATE_UNIT terms, that tie is the normal
    /// case — so the first-listed stablecoin is effectively the default anchor,
    /// almost regardless of the actual values. Swapping the order changes which
    /// stablecoin becomes the USDT/USD anchor, even when both are healthy.
    ///
    /// Implication: with two symbols, list the most-trusted coin FIRST. (With an
    /// odd set of >= 3 the true middle is selected by value and order no longer
    /// matters — see `three_symbol_selection_is_order_independent`.)
    #[test]
    fn two_symbol_selection_is_order_dependent_on_ties() {
        let make = |symbol: &str, rate: u64| {
            QueriedExchangeRate::new(
                Asset {
                    symbol: symbol.to_string(),
                    class: AssetClass::Cryptocurrency,
                },
                Asset {
                    symbol: "USDT".to_string(),
                    class: AssetClass::Cryptocurrency,
                },
                1647734400,
                &[rate],
                1,
                1,
                None,
            )
        };
        // 1.001 and 0.999 — both healthy, sum is even, so the two are exactly
        // equidistant from their midpoint (a tie).
        let high = make("HIGH", 1_001_000_000);
        let low = make("LOW", 999_000_000);

        let first_high =
            get_stablecoin_rate(&[high.clone(), low.clone()], &crate::api::usd_asset())
                .expect("a rate should be returned");
        let first_low =
            get_stablecoin_rate(&[low, high], &crate::api::usd_asset()).expect("a rate");

        // Order alone changed the selected stablecoin, despite identical inputs.
        assert_ne!(first_high.rates, first_low.rates);
        // [HIGH, LOW] picked HIGH (1.001) -> USDT/USD = 1/1.001 < 1.
        assert!(median(&first_high.rates) < RATE_UNIT);
        // [LOW, HIGH] picked LOW (0.999) -> USDT/USD = 1/0.999 > 1.
        assert!(median(&first_low.rates) > RATE_UNIT);
    }

    /// Companion to the two-symbol test: with an ODD set of three the selection
    /// is the TRUE MIDDLE by value, so it is INDEPENDENT of input order. The
    /// same three medians in any order yield the same selected (middle) rate.
    #[test]
    fn three_symbol_selection_is_order_independent() {
        let make = |symbol: &str, rate: u64| {
            QueriedExchangeRate::new(
                Asset {
                    symbol: symbol.to_string(),
                    class: AssetClass::Cryptocurrency,
                },
                Asset {
                    symbol: "USDT".to_string(),
                    class: AssetClass::Cryptocurrency,
                },
                1647734400,
                &[rate],
                1,
                1,
                None,
            )
        };
        let low = make("LOW", 950_000_000); // 0.95 (e.g. a depeg)
        let mid = make("MID", 1_000_000_000); // 1.00
        let high = make("HIGH", 1_002_000_000); // 1.002

        // The middle value (1.00) is selected regardless of order, so the
        // depegged 0.95 outlier can never win.
        let usd = crate::api::usd_asset();
        let a = get_stablecoin_rate(&[low.clone(), mid.clone(), high.clone()], &usd).expect("a");
        let b = get_stablecoin_rate(&[high, low, mid], &usd).expect("b");
        assert_eq!(a.rates, b.rates);
        // Selected middle = 1.00 -> USDT/USD = 1/1.00 = RATE_UNIT.
        assert_eq!(median(&a.rates), RATE_UNIT);
    }
}
