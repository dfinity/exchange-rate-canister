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

#[allow(dead_code)]
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
