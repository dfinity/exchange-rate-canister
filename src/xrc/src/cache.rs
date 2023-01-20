extern crate lru;
use lru::LruCache;
use std::num::NonZeroUsize;

use crate::candid::AssetClass::Cryptocurrency;
use crate::{usdt_asset, QueriedExchangeRate, RATE_UNIT, USDT};

/// The [ExchangeRateCache] caches exchange rates.
/// It always expects the USDT cryptocurrency asset as the quote asset.
pub(crate) struct ExchangeRateCache {
    /// The internal LRU cache.
    lru_cache: LruCache<(String, u64), QueriedExchangeRate>,
}
impl ExchangeRateCache {
    /// The function creates an [ExchangeRateCache] with a certain maximum size.
    pub(crate) fn new(capacity: usize) -> Self {
        ExchangeRateCache {
            lru_cache: LruCache::new(NonZeroUsize::new(capacity).unwrap()),
        }
    }

    /// The function inserts the given exchange rate.
    /// The base asset must be a cryptocurrency that is not USDT and the quote asset must be USDT.
    /// If one of these conditions does not hold, the rate is not cached.
    pub(crate) fn insert(&mut self, rate: &QueriedExchangeRate) {
        if rate.base_asset.symbol.to_uppercase() != USDT
            && rate.base_asset.class == Cryptocurrency
            && rate.quote_asset == usdt_asset()
        {
            self.lru_cache.put(
                (rate.base_asset.symbol.to_uppercase(), rate.timestamp),
                rate.clone(),
            );
        }
    }

    /// The function returns the cached exchange rate, if any, for the given base asset symbol
    /// and timestamp.
    /// If the base asset symbol is USDT, the USDT/USDT rate of 1.0 is returned.
    pub(crate) fn get(&mut self, symbol: &str, timestamp: u64) -> Option<QueriedExchangeRate> {
        if symbol.to_uppercase() == USDT {
            Some(QueriedExchangeRate::new(
                usdt_asset(),
                usdt_asset(),
                timestamp,
                &[RATE_UNIT],
                0,
                0,
                None,
            ))
        } else {
            self.lru_cache
                .get(&(symbol.to_uppercase(), timestamp))
                .cloned()
        }
    }

    /// The function returns the number of cached exchange rates.
    pub(crate) fn len(&self) -> usize {
        self.lru_cache.len()
    }
}

#[cfg(test)]
mod test {
    use crate::api::usd_asset;
    use crate::cache::ExchangeRateCache;
    use crate::candid::{Asset, AssetClass};
    use crate::{usdt_asset, QueriedExchangeRate, RATE_UNIT};

    /// The function verifies that the exchange rate for a cryptocurrency base asset that is not
    /// USDT is cached correctly.
    #[test]
    fn cache_stores_cryptocurrency_rate() {
        let mut cache = ExchangeRateCache::new(10);
        let inserted_rate = QueriedExchangeRate::new(
            Asset {
                symbol: "ICP".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            usdt_asset(),
            0,
            &[100 * RATE_UNIT],
            8,
            8,
            None,
        );
        cache.insert(&inserted_rate);
        let cached_rate = cache.get("icp", 0);
        assert_eq!(cache.len(), 1);
        assert!(matches!(cached_rate, Some(rate) if rate == inserted_rate));
    }

    /// The function verifies that the cache returns the USDT/USDT rate of 1.0 for an arbitrary timestamp.
    #[test]
    fn cache_returns_usdt_rate() {
        let mut cache = ExchangeRateCache::new(10);
        let timestamp = 123456789;
        let cached_rate = cache.get("usdt", timestamp);
        let expected_rate = QueriedExchangeRate::new(
            usdt_asset(),
            usdt_asset(),
            timestamp,
            &[RATE_UNIT],
            0,
            0,
            None,
        );
        assert!(matches!(cached_rate, Some(rate) if rate == expected_rate));
    }

    /// The function verifies that fiat rates are not cached.
    #[test]
    fn cache_does_not_store_fiat_rates() {
        let mut cache = ExchangeRateCache::new(10);
        cache.insert(&QueriedExchangeRate::new(
            usd_asset(),
            usdt_asset(),
            0,
            &[RATE_UNIT],
            0,
            0,
            None,
        ));
        assert_eq!(cache.len(), 0);
    }

    /// The function verifies that USDT rates are not cached.
    #[test]
    fn cache_does_not_store_usdt_rate() {
        let mut cache = ExchangeRateCache::new(10);
        cache.insert(&QueriedExchangeRate::new(
            usdt_asset(),
            usdt_asset(),
            0,
            &[RATE_UNIT],
            0,
            0,
            None,
        ));
        assert_eq!(cache.len(), 0);
    }
}
