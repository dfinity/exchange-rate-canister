use std::{cell::RefCell, collections::HashSet};

use crate::{
    candid::{Asset, ExchangeRateError},
    QueriedExchangeRate,
};

/// A key contains the symbol and the timestamp.
type Key = (String, u64);
///
type InflightCryptoUsdtRequests = HashSet<Key>;

thread_local! {
    /// Contains the symbol-timestamp pairs that are currently being requested using HTTP outcalls.
    static INFLIGHT_CRYPTO_USDT_RATE_REQUESTS: RefCell<InflightCryptoUsdtRequests> = RefCell::new(HashSet::new());
}

/// Checks if the symbol-timestamp pair is in the set.
fn contains(key: &Key) -> bool {
    INFLIGHT_CRYPTO_USDT_RATE_REQUESTS.with(|cell| cell.borrow().contains(key))
}

/// Adds a symbol-timestamp pair to the set.
fn add(key: Key) {
    INFLIGHT_CRYPTO_USDT_RATE_REQUESTS.with(|cell| {
        cell.borrow_mut().insert(key);
    });
}

/// Removes a symbol-timestamp pair from the set.
fn remove(key: &Key) {
    INFLIGHT_CRYPTO_USDT_RATE_REQUESTS.with(|cell| {
        cell.borrow_mut().remove(key);
    });
}

/// Provides a simple interface for the rest of the canister to be able to check
/// if an asset-timestamp pair is in the state.
pub(crate) fn is_inflight(asset: &Asset, timestamp: u64) -> bool {
    let key = (asset.symbol.clone(), timestamp);
    contains(&key)
}

/// Used to wrap around the HTTP outcalls so that the canister can avoid sending
/// similar requests to crypto exchanges.
pub(crate) async fn with_inflight_tracking<F>(
    symbols: Vec<String>,
    timestamp: u64,
    future: F,
) -> Result<QueriedExchangeRate, ExchangeRateError>
where
    F: std::future::Future<Output = Result<QueriedExchangeRate, ExchangeRateError>>,
{
    // Need to set the guard to maintain the lifetime until the future is complete.
    let _guard = InflightCryptoUsdtRequestsGuard::new(symbols, timestamp);
    future.await
}

/// Guard to ensure that the tracking set adds and removes symbol-timestamp pairs
/// correctly.
struct InflightCryptoUsdtRequestsGuard {
    symbols: Vec<String>,
    timestamp: u64,
}

impl InflightCryptoUsdtRequestsGuard {
    /// Adds all symbols paired to a given timestamp to the tracking set.
    fn new(symbols: Vec<String>, timestamp: u64) -> Self {
        for symbol in &symbols {
            add((symbol.clone(), timestamp));
        }
        Self { symbols, timestamp }
    }
}

impl Drop for InflightCryptoUsdtRequestsGuard {
    /// Removes all symbols paired to a given timestamp to the tracking set.
    fn drop(&mut self) {
        for symbol in &self.symbols {
            remove(&(symbol.clone(), self.timestamp));
        }
    }
}

#[cfg(test)]
mod test {

    use futures::FutureExt;

    use crate::candid::AssetClass;

    use super::*;

    /// The function verifies that when a rate is returned from the provided async block,
    /// the guard correctly releases the symbol-timestamp pair from the set.
    #[test]
    fn with_inflight_tracking_with_ok_result_returned() {
        let rate =
            with_inflight_tracking(vec!["ICP".to_string(), "BTC".to_string()], 0, async move {
                assert!(contains(&("ICP".to_string(), 0)));
                assert!(contains(&("BTC".to_string(), 0)));
                Ok(QueriedExchangeRate::default())
            })
            .now_or_never()
            .expect("should succeed")
            .expect("rate should be in result");
        assert_eq!(rate, QueriedExchangeRate::default());
        assert!(!contains(&("ICP".to_string(), 0)));
        assert!(!contains(&("BTC".to_string(), 0)));
    }

    /// The function verifies that when an error is returned from the provided async block,
    /// the guard correctly releases the symbol-timestamp pair from the set.
    #[test]
    fn with_inflight_tracking_with_error_result_returned() {
        let err =
            with_inflight_tracking(vec!["ICP".to_string(), "BTC".to_string()], 0, async move {
                assert!(contains(&("ICP".to_string(), 0)));
                assert!(contains(&("BTC".to_string(), 0)));
                Err(ExchangeRateError::CryptoBaseAssetNotFound)
            })
            .now_or_never()
            .expect("should succeed")
            .expect_err("error should be in result");
        assert!(matches!(err, ExchangeRateError::CryptoBaseAssetNotFound));
        assert!(!contains(&("ICP".to_string(), 0)));
        assert!(!contains(&("BTC".to_string(), 0)));
    }

    /// The function verifies that if the symbol-timestamp pair is not in the tracking set,
    /// then the request is not pending.
    #[test]
    fn is_inflight_checks_if_symbol_timestamp_is_not_in_set() {
        let asset = Asset {
            symbol: "ICP".to_string(),
            class: AssetClass::Cryptocurrency,
        };
        assert!(!is_inflight(&asset, 0));
    }

    /// The function verifies that if the symbol-timestamp pair is in the tracking set,
    /// then the request is pending.
    #[test]
    fn is_inflight_checks_if_symbol_timestamp_is_in_set() {
        let asset = Asset {
            symbol: "ICP".to_string(),
            class: AssetClass::Cryptocurrency,
        };
        add((asset.symbol.clone(), 0));
        assert!(is_inflight(&asset, 0));
    }
}
