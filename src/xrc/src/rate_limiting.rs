use crate::{candid::ExchangeRateError, QueriedExchangeRate, RATE_LIMITING_REQUEST_COUNTER};

/// A limit for how many HTTP requests the exchange rate canister may issue at any given time.
/// The request counter is not allowed to go over this limit.
const REQUEST_COUNTER_LIMIT: usize = 56;

/// This function is used to wrap HTTP outcalls so that the requests can be rate limited.
/// If the caller is the CMC, it will ignore the rate limiting.
pub(crate) async fn with_request_counter<F>(
    http_requests_needed: usize,
    future: F,
) -> Result<QueriedExchangeRate, ExchangeRateError>
where
    F: std::future::Future<Output = Result<QueriedExchangeRate, ExchangeRateError>>,
{
    // Need to set the guard to maintain the lifetime until the future is complete.
    let _guard = RateLimitingRequestCounterGuard::new(http_requests_needed);
    future.await
}

/// Checks that a request can be made.
pub(crate) fn is_rate_limited(requests_needed: usize) -> bool {
    let request_counter = get_request_counter();
    requests_needed.saturating_add(request_counter) > REQUEST_COUNTER_LIMIT
}

/// Returns the value of the request counter.
pub(crate) fn get_request_counter() -> usize {
    RATE_LIMITING_REQUEST_COUNTER.with(|cell| cell.get())
}

/// Guard to ensure the rate limiting request counter is incremented and decremented properly.
struct RateLimitingRequestCounterGuard {
    http_requests_needed: usize,
}

impl RateLimitingRequestCounterGuard {
    /// Increment the counter and return the guard.
    fn new(http_requests_needed: usize) -> Self {
        RATE_LIMITING_REQUEST_COUNTER.with(|cell| {
            let value = cell.get();
            let value = value.saturating_add(http_requests_needed);
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
            let value = cell.get();
            let value = value.saturating_sub(self.http_requests_needed);
            cell.set(value);
        });
    }
}

#[cfg(test)]
mod test {
    use futures::FutureExt;

    use super::*;

    /// The function verifies that when a rate is returned from the provided async
    /// block, the counter increments and decrements correctly.
    #[test]
    fn with_request_counter_with_ok_result_returned() {
        let http_requests_needed = 3;
        let rate = with_request_counter(http_requests_needed, async move {
            assert_eq!(get_request_counter(), http_requests_needed);
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
        let http_requests_needed = 3;
        let error = with_request_counter(http_requests_needed, async move {
            assert_eq!(get_request_counter(), http_requests_needed);
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
        RATE_LIMITING_REQUEST_COUNTER.with(|c| c.set(54));
        assert!(is_rate_limited(3));
    }
}
