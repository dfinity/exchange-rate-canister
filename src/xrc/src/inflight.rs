use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
};

use crate::{candid::ExchangeRateError, QueriedExchangeRate};

type InflightCryptoUsdtRequests = HashSet<(String, u64)>;

thread_local! {
    static INFLIGHT_CRYPTO_USDT_RATE_REQUESTS: RefCell<InflightCryptoUsdtRequests> = RefCell::new(HashSet::new());
}

fn contains(key: &(String, u64)) -> bool {
    INFLIGHT_CRYPTO_USDT_RATE_REQUESTS.with(|cell| cell.borrow().contains(key))
}

fn add(key: (String, u64)) {
    INFLIGHT_CRYPTO_USDT_RATE_REQUESTS.with(|cell| {
        cell.borrow_mut().insert(key);
    });
}

fn remove(key: &(String, u64)) {
    INFLIGHT_CRYPTO_USDT_RATE_REQUESTS.with(|cell| {
        cell.borrow_mut().remove(key);
    });
}

pub(crate) async fn with_crypto_exchanges<F>(
    symbol: String,
    timestamp: u64,
    future: F,
) -> Result<QueriedExchangeRate, ExchangeRateError>
where
    F: std::future::Future<Output = Result<QueriedExchangeRate, ExchangeRateError>>,
{
    let key = (symbol, timestamp);
    if contains(&key) {
        return Err(ExchangeRateError::RateLimited);
    }

    let _guard = InflightCryptoUsdtRequestsGuard::new(key);
    future.await
}

struct InflightCryptoUsdtRequestsGuard {
    key: (String, u64),
}

impl InflightCryptoUsdtRequestsGuard {
    fn new(key: (String, u64)) -> Self {
        add(key.clone());
        Self { key }
    }
}

impl Drop for InflightCryptoUsdtRequestsGuard {
    fn drop(&mut self) {
        remove(&self.key);
    }
}
