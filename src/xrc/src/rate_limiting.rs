use crate::{
    candid::ExchangeRateError, QueriedExchangeRate, EXCHANGES, RATE_LIMITING_REQUEST_COUNTER,
};

/// A limit for how many HTTP requests the exchange rate canister may issue at any given time.
/// The request counter is not allowed to go over this limit.
const REQUEST_COUNTER_LIMIT: usize = 56;

/// This function is used to wrap HTTP outcalls so that the requests can be rate limited.
/// If the caller is the CMC, it will ignore the rate limiting.
pub(crate) async fn with_request_counter<F>(
    num_rates_needed: usize,
    future: F,
) -> Result<QueriedExchangeRate, ExchangeRateError>
where
    F: std::future::Future<Output = Result<QueriedExchangeRate, ExchangeRateError>>,
{
    // Need to set the guard to maintain the lifetime until the future is complete.
    let _guard = RateLimitingRequestCounterGuard::new(num_rates_needed);
    future.await
}

/// Checks that a request can be made.
pub(crate) fn is_rate_limited(num_rates_needed: usize) -> bool {
    let request_counter = get_request_counter();
    let available_exchanges_count = available_exchanges_count();
    let http_requests_needed = available_exchanges_count.saturating_mul(num_rates_needed);
    http_requests_needed.saturating_add(request_counter) > REQUEST_COUNTER_LIMIT
}

/// Returns the value of the request counter.
pub(crate) fn get_request_counter() -> usize {
    RATE_LIMITING_REQUEST_COUNTER.with(|cell| cell.get())
}

fn available_exchanges_count() -> usize {
    EXCHANGES.iter().filter(|e| e.is_available()).count()
}

/// Guard to ensure the rate limiting request counter is incremented and decremented properly.
struct RateLimitingRequestCounterGuard {
    http_requests_needed: usize,
}

impl RateLimitingRequestCounterGuard {
    /// Increment the counter and return the guard.
    fn new(num_rates_needed: usize) -> Self {
        let available_exchanges_count = available_exchanges_count();
        let http_requests_needed = available_exchanges_count.saturating_mul(num_rates_needed);
        RATE_LIMITING_REQUEST_COUNTER.with(|cell| {
            let value = cell.get().saturating_add(http_requests_needed);
            cell.set(value);
        });
        Self {
            http_requests_needed,
        }
    }
}

impl Drop for RateLimitingRequestCounterGuard {
    /// Decrement the counter when guard is dropped.
    fn drop(&mut self) {
        RATE_LIMITING_REQUEST_COUNTER.with(|cell| {
            let value = cell.get().saturating_sub(self.http_requests_needed);
            cell.set(value);
        });
    }
}

#[cfg(test)]
pub(crate) mod test {
    use futures::FutureExt;

    use super::*;

    pub(crate) fn set_request_counter(requests: usize) {
        RATE_LIMITING_REQUEST_COUNTER.with(|c| c.set(requests));
    }

    /// The function verifies that when a rate is returned from the provided async
    /// block, the counter increments and decrements correctly.
    #[test]
    fn with_request_counter_with_ok_result_returned() {
        let num_rates_needed = 2;
        let rate = with_request_counter(num_rates_needed, async move {
            assert_eq!(
                get_request_counter(),
                num_rates_needed * available_exchanges_count()
            );
            Ok(QueriedExchangeRate::default())
        })
        .now_or_never()
        .expect("should succeed")
        .expect("rate should be in result");
        assert_eq!(rate, QueriedExchangeRate::default());
        assert_eq!(get_request_counter(), 0);
    }

    /// The function verifies that when an error occurs in the provided async
    /// block, the counter increments and decrements correctly.
    #[test]
    fn with_request_counter_with_error_returned() {
        let num_rates_needed = 2;
        let error = with_request_counter(num_rates_needed, async move {
            assert_eq!(
                get_request_counter(),
                num_rates_needed * available_exchanges_count()
            );
            Err(ExchangeRateError::StablecoinRateNotFound)
        })
        .now_or_never()
        .expect("should succeed")
        .expect_err("error should be in result");

        assert!(matches!(error, ExchangeRateError::StablecoinRateNotFound));
        assert_eq!(get_request_counter(), 0);
    }

    /// The function verifies that if the limit has not been exceeded,
    /// then the request is not rate limited.
    #[test]
    fn is_rate_limited_when_counter_is_below_limit() {
        assert!(!is_rate_limited(1));
    }

    /// The function verifies that if the limit will be exceeded,
    /// then the request is rate limited.
    #[test]
    fn is_rate_limited_checks_against_a_hard_limit() {
        set_request_counter(52);
        assert!(is_rate_limited(2));
    }
}
