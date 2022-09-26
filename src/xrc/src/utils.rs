const NANOS_PER_SEC: u64 = 1_000_000_000;

/// Gets the current time in seconds.
pub fn time_secs() -> u64 {
    let now = ic_cdk::api::time();
    now / NANOS_PER_SEC
}

/// The function returns the median of the provided values.
pub fn get_median(values: &mut [u64]) -> u64 {
    values.sort();

    let length = values.len();
    if length % 2 == 0 {
        (values[(length / 2) - 1] + values[length / 2]) / 2
    } else {
        values[length / 2]
    }
}
