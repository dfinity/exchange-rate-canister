use crate::{environment::Environment, PRIVILEGED_CANISTER_IDS, RATE_UNIT};
use ic_cdk::export::Principal;
use ic_xrc_types::{Asset, GetExchangeRateRequest};

const NANOS_PER_SEC: u64 = 1_000_000_000;

/// Gets the current time in seconds.
pub(crate) fn time_secs() -> u64 {
    let now = ic_cdk::api::time();
    now / NANOS_PER_SEC
}

/// The function returns the median of the provided values.
pub(crate) fn median(values: &[u64]) -> u64 {
    let mut copied_values = values.to_vec();
    copied_values.sort();

    let length = copied_values.len();
    if length % 2 == 0 {
        (copied_values[(length / 2) - 1] + copied_values[length / 2]) / 2
    } else {
        copied_values[length / 2]
    }
}

/// The function returns the median of the provided values with the condition that
/// the median must be among the values.
/// If the number of values is odd, the result is identical to the output of [median].
/// Otherwise, the value closest to the true median is returned.
pub(crate) fn median_in_set(values: &[u64]) -> u64 {
    let median = median(values);
    if values.len() % 2 == 0 {
        let mut current_median = 0;
        let mut current_diff = u64::MAX;
        for value in values {
            let diff = value.abs_diff(median);
            if diff < current_diff {
                current_median = *value;
                current_diff = diff;
            }
        }
        current_median
    } else {
        median
    }
}

/// The function computes the standard deviation of the
/// given rates.
pub(crate) fn standard_deviation(rates: &[u64]) -> u64 {
    let count = rates.len() as u64;

    // There is no deviation if there are fewer than 2 rates.
    if count < 2 {
        return 0;
    }

    let mean: i64 = (rates.iter().sum::<u64>() / count) as i64;
    let variance = rates
        .iter()
        .map(|rate| (((*rate as i64).saturating_sub(mean)).pow(2)) as u64)
        .sum::<u64>()
        / (count - 1);
    (variance as f64).sqrt() as u64
}

/// Pulls the timestamp from a rate request. If the timestamp is not set,
/// pulls the latest IC time and normalizes the timestamp by setting it to the
/// start of the most recent minute if 30 seconds or more have already passed. Otherwise,
/// the timestamp is set to the start of the previous minute.
pub(crate) fn get_normalized_timestamp(
    env: &impl Environment,
    request: &GetExchangeRateRequest,
) -> u64 {
    (request
        .timestamp
        .unwrap_or_else(|| env.time_secs().saturating_sub(30))
        / 60)
        * 60
}

/// Sanitizes a [GetExchangeRateRequest] to clean up the following:
/// * base asset symbol - should be uppercase
/// * quote asset symbol - should be uppercase
pub(crate) fn sanitize_request(request: &GetExchangeRateRequest) -> GetExchangeRateRequest {
    let base_asset_symbol = if request.base_asset.symbol.chars().all(char::is_alphanumeric) {
        request.base_asset.symbol.to_uppercase()
    } else {
        "".to_string()
    };

    let quote_asset_symbol = if request
        .quote_asset
        .symbol
        .chars()
        .all(char::is_alphanumeric)
    {
        request.quote_asset.symbol.to_uppercase()
    } else {
        "".to_string()
    };

    GetExchangeRateRequest {
        base_asset: Asset {
            symbol: base_asset_symbol,
            class: request.base_asset.class.clone(),
        },
        quote_asset: Asset {
            symbol: quote_asset_symbol,
            class: request.quote_asset.class.clone(),
        },
        timestamp: request.timestamp,
    }
}

/// Checks if the caller's principal ID is anonymous.
pub(crate) fn is_caller_anonymous(caller: &Principal) -> bool {
    *caller == Principal::anonymous()
}

/// Checks if the caller's principal ID belongs to the Cycles Minting Canister.
pub(crate) fn is_caller_privileged(caller: &Principal) -> bool {
    PRIVILEGED_CANISTER_IDS.contains(caller)
}

/// Inverts a given rate. If the rate cannot be inverted, return None.
pub(crate) fn checked_invert_rate(rate: u64) -> Option<u64> {
    (RATE_UNIT * RATE_UNIT).checked_div(rate)
}

/// Checks if the canister is supporting IPv4 exchanges and forex sources.
pub(crate) fn is_ipv4_support_available() -> bool {
    cfg!(feature = "ipv4-support")
}

#[cfg(test)]
pub(crate) mod test {
    use std::path::PathBuf;

    use ic_xrc_types::AssetClass;

    use super::*;

    pub(crate) fn load_file(path: &str) -> Vec<u8> {
        std::fs::read(PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join(path))
            .expect("failed to read file")
    }

    #[test]
    fn cycles_minting_canister_id_is_correct() {
        let principal_from_text = Principal::from_text("rkp4c-7iaaa-aaaaa-aaaca-cai")
            .expect("should be a valid textual principal ID");
        assert!(is_caller_privileged(&principal_from_text));
    }

    #[test]
    fn nns_dapp_id_is_correct() {
        let principal_from_text = Principal::from_text("qoctq-giaaa-aaaaa-aaaea-cai")
            .expect("should be a valid textual principal ID");
        assert!(is_caller_privileged(&principal_from_text));
    }

    #[test]
    fn tvl_dapp_id_is_correct() {
        let principal_from_text = Principal::from_text("ewh3f-3qaaa-aaaap-aazjq-cai")
            .expect("should be a valid textual principal ID");
        assert!(is_caller_privileged(&principal_from_text));
    }

    #[test]
    fn sanitize_request_uppercases_the_asset_symbols_and_copies_the_other_properties() {
        let request = GetExchangeRateRequest {
            base_asset: Asset {
                symbol: "icp".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            quote_asset: Asset {
                symbol: "eur".to_string(),
                class: AssetClass::FiatCurrency,
            },
            timestamp: Some(1234),
        };

        let sanitized_request = sanitize_request(&request);

        assert_eq!(sanitized_request.base_asset.symbol, "ICP");
        assert_eq!(
            sanitized_request.base_asset.class,
            AssetClass::Cryptocurrency
        );
        assert_eq!(sanitized_request.quote_asset.symbol, "EUR");
        assert_eq!(
            sanitized_request.quote_asset.class,
            AssetClass::FiatCurrency
        );
        assert_eq!(request.timestamp, Some(1234));
    }

    #[test]
    fn sanitize_request_cleans_out_symbols_with_invalid_characters() {
        let request = GetExchangeRateRequest {
            base_asset: Asset {
                symbol: "<!@#@!>".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            quote_asset: Asset {
                symbol: "<IMG SRC=j&#X41vascript:alert('test2')>".to_string(),
                class: AssetClass::FiatCurrency,
            },
            timestamp: Some(1234),
        };

        let sanitized_request = sanitize_request(&request);

        assert_eq!(sanitized_request.base_asset.symbol, "");
        assert_eq!(
            sanitized_request.base_asset.class,
            AssetClass::Cryptocurrency
        );
        assert_eq!(sanitized_request.quote_asset.symbol, "");
        assert_eq!(
            sanitized_request.quote_asset.class,
            AssetClass::FiatCurrency
        );
        assert_eq!(request.timestamp, Some(1234));
    }
}
