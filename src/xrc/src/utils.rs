use crate::{candid::GetExchangeRateRequest, MAINNET_CYCLES_MINTING_CANISTER_ID};
use ic_cdk::export::Principal;

const NANOS_PER_SEC: u64 = 1_000_000_000;

/// Gets the current time in seconds.
pub fn time_secs() -> u64 {
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

/// The function computes the scaled (permyriad) standard deviation of the
/// given rates.
pub(crate) fn standard_deviation_permyriad(rates: &[u64]) -> u64 {
    let sum: u64 = rates.iter().sum();
    let count = rates.len() as u64;
    let mean: i64 = (sum / count) as i64;
    let variance = rates
        .iter()
        .map(|rate| (((*rate as i64).saturating_sub(mean)).pow(2)) as u64)
        .sum::<u64>()
        / count;
    // Note that the variance has a scaling factor of 10_000^2.
    // The square root reduces the scaling factor back to 10_000.
    (variance as f64).sqrt() as u64
}

/// Pulls the timestamp from a rate request. If the timestamp is not set,
/// pulls the latest IC time and normalizes the timestamp to the most recent
/// minute.
pub fn get_normalized_timestamp(request: &GetExchangeRateRequest) -> u64 {
    (request.timestamp.unwrap_or_else(time_secs) / 60) * 60
}

/// Checks if the caller's principal ID belongs to the Cycles Minting Canister.
pub fn is_caller_the_cmc(caller: &Principal) -> bool {
    *caller == MAINNET_CYCLES_MINTING_CANISTER_ID
}

/// Inverts a given rate.
pub fn invert_rate(rate: u64) -> u64 {
    100_000_000 / rate
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn cycles_minting_canister_id_is_correct() {
        let principal_from_text = Principal::from_text("rkp4c-7iaaa-aaaaa-aaaca-cai")
            .expect("should be a valid textual principal ID");
        assert!(is_caller_the_cmc(&principal_from_text));
    }
}
