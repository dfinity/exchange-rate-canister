use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
};

use crate::{
    candid::{Asset, ExchangeRateError},
    CallExchangeError, QueriedExchangeRate,
};

type Key = (String, u64);
type InflightCryptoUsdtRequests = HashSet<Key>;

thread_local! {
    static INFLIGHT_CRYPTO_USDT_RATE_REQUESTS: RefCell<InflightCryptoUsdtRequests> = RefCell::new(HashSet::new());
}

fn contains(key: &Key) -> bool {
    INFLIGHT_CRYPTO_USDT_RATE_REQUESTS.with(|cell| cell.borrow().contains(key))
}

fn contains_any(symbols: &[String], timestamp: u64) -> bool {
    INFLIGHT_CRYPTO_USDT_RATE_REQUESTS.with(|cell| {
        let borrowed = cell.borrow();
        symbols
            .iter()
            .any(|symbol| borrowed.contains(&(symbol.clone(), timestamp)))
    })
}

fn add(key: Key) {
    INFLIGHT_CRYPTO_USDT_RATE_REQUESTS.with(|cell| {
        cell.borrow_mut().insert(key);
    });
}

fn remove(key: &Key) {
    INFLIGHT_CRYPTO_USDT_RATE_REQUESTS.with(|cell| {
        cell.borrow_mut().remove(key);
    });
}

pub(crate) fn is_inflight(asset: &Asset, timestamp: u64) -> bool {
    let key = (asset.symbol.clone(), timestamp);
    contains(&key)
}

pub(crate) async fn with_inflight<F>(
    symbols: Vec<String>,
    timestamp: u64,
    future: F,
) -> Result<QueriedExchangeRate, ExchangeRateError>
where
    F: std::future::Future<Output = Result<QueriedExchangeRate, ExchangeRateError>>,
{
    if contains_any(&symbols, timestamp) {
        return Err(ExchangeRateError::Pending);
    }

    let _guard = NewInflightCryptoUsdtRequestsGuard::new(symbols, timestamp);
    future.await
}

struct NewInflightCryptoUsdtRequestsGuard {
    symbols: Vec<String>,
    timestamp: u64,
}

impl NewInflightCryptoUsdtRequestsGuard {
    fn new(symbols: Vec<String>, timestamp: u64) -> Self {
        for symbol in &symbols {
            add((symbol.clone(), timestamp));
        }
        Self { symbols, timestamp }
    }
}

impl Drop for NewInflightCryptoUsdtRequestsGuard {
    fn drop(&mut self) {
        for symbol in &self.symbols {
            remove(&(symbol.clone(), self.timestamp));
        }
    }
}
