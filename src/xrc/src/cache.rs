//! The cache is used to temporarily store cryptocurrency exchange in order to serve them
//! from the cache whenever possible.
//! Cached rates expire and are removed from the cache automatically.

use crate::candid::AssetClass;
use crate::{Asset, QueriedExchangeRate};
use std::collections::BTreeMap;

/// Type to identify logical time values used in the pruning mechanism.
type LogicalTime = u64;

#[derive(Clone, Debug)]
struct CachedExchangeRate {
    rate: QueriedExchangeRate,
    logical_time: LogicalTime,
    expiration_time: u64,
}

impl CachedExchangeRate {
    /// The function created a [CachedExchangeRate] instance.
    fn new(rate: QueriedExchangeRate, logical_time: LogicalTime, expiration_time: u64) -> Self {
        CachedExchangeRate {
            rate,
            logical_time,
            expiration_time,
        }
    }
}

/// The struct used to cache exchange rates.
#[derive(Clone, Debug)]
pub(crate) struct ExchangeRateCache {
    /// The constant quote asset symbol of all cached rates.
    quote_asset_symbol: String,
    /// The soft maximum cache size. If the hard maximum size is reached, it is reduced at least
    /// down to the soft maximum size.
    soft_max_size: usize,
    /// The hard maximum cache size. A clean-up is triggered when this size is reached, evicting
    /// cache element that are expired or have not been accessed recently.
    hard_max_size: usize,
    /// Logical time to implement an LRU eviction policy.
    logical_time: u64,
    /// The cached cryptocurrency rates, indexed by cryptocurrency symbol.
    rates: BTreeMap<String, Vec<CachedExchangeRate>>,
    /// The total number of cached rates.
    size: usize,
}

impl ExchangeRateCache {
    /// The function creates an [ExchangeRateCache] instance.
    #[allow(dead_code)]
    pub(crate) fn new(
        quote_asset_symbol: String,
        soft_max_size: usize,
        hard_max_size: usize,
    ) -> Self {
        ExchangeRateCache {
            quote_asset_symbol,
            soft_max_size,
            hard_max_size,
            logical_time: 0,
            rates: BTreeMap::new(),
            size: 0,
        }
    }

    /// The function returns the quote asset used for all cached rates.
    fn get_quote_asset(&self) -> Asset {
        Asset {
            symbol: self.quote_asset_symbol.to_string(),
            class: AssetClass::Cryptocurrency,
        }
    }

    /// The given rate is inserted into the cache at the provided real time.
    /// The function returns an error if the quote is not the expected quote asset for this cache.
    #[allow(dead_code)]
    pub(crate) fn insert(
        &mut self,
        rate: QueriedExchangeRate,
        time: u64,
        expiration_time: u64,
    ) -> Result<(), String> {
        // Make sure that the quote asset is correct.
        if rate.quote_asset != self.get_quote_asset() {
            return Err(format!("Invalid quote asset: {}", rate.quote_asset.symbol));
        }

        let symbol = rate.base_asset.symbol.clone();
        let timestamp = rate.timestamp;

        let rates_option = self.rates.get_mut(&symbol);

        let new_rate = CachedExchangeRate::new(rate, self.logical_time, expiration_time);

        match rates_option {
            Some(rates) => {
                let old_size = rates.len();
                rates.retain(|c| c.expiration_time > time && c.rate.timestamp != timestamp);
                rates.push(new_rate);
                let new_size = rates.len();
                self.size = (self.size + new_size) - old_size;
            }
            None => {
                let rates = vec![new_rate];
                self.rates.insert(symbol.to_string(), rates);
                self.size += 1;
            }
        };
        self.logical_time += 1;

        if self.size >= (self.hard_max_size as usize) {
            self.prune();
        }
        Ok(())
    }

    /// The function prunes the cache by removing cache entries until `soft_max_size`
    /// entries remain.
    fn prune(&mut self) {
        let mut logical_times = vec![];
        for rates in self.rates.values() {
            for rate in rates {
                logical_times.push(rate.logical_time);
            }
        }
        logical_times.sort();
        let cut_off_time = logical_times[self.size - self.soft_max_size];
        // Keep all rates with a logical time at least `cut_off_time`.
        for rates in self.rates.values_mut() {
            let old_size = rates.len();
            rates.retain(|c| c.logical_time >= cut_off_time);
            let new_size = rates.len();
            self.size -= old_size - new_size;
        }
    }

    /// The function returns the total size of the cache.
    #[allow(dead_code)]
    pub(crate) fn size(&self) -> usize {
        self.size
    }

    /// The function returns the cached exchange rate for the given asset symbol and timestamp
    /// at the provided real time.
    #[allow(dead_code)]
    pub(crate) fn get(
        &mut self,
        symbol: &str,
        timestamp: u64,
        time: u64,
    ) -> Option<QueriedExchangeRate> {
        // If the symbol is the quote asset symbol itself, a rate of 1.00 is returned.
        if symbol == self.quote_asset_symbol {
            return Some(QueriedExchangeRate {
                base_asset: self.get_quote_asset(),
                quote_asset: self.get_quote_asset(),
                timestamp,
                rates: vec![10_000],
                num_queried_sources: 0,
                num_received_rates: 0,
            });
        };
        match self.rates.get_mut(symbol) {
            Some(rates) => {
                let old_size = rates.len();
                rates.retain(|c| c.expiration_time > time);
                let new_size = rates.len();
                self.size = (self.size + new_size) - old_size;
                let cached_rate_option = rates.iter_mut().find(|c| c.rate.timestamp == timestamp);
                match cached_rate_option {
                    Some(cached_rate) => {
                        cached_rate.logical_time = self.logical_time;
                        self.logical_time += 1;
                        Some(cached_rate.rate.clone())
                    }
                    None => None,
                }
            }
            None => None,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::candid::AssetClass;
    use crate::{Asset, USDT};

    /// The function returns a basic exchange rate collection struct to be used in tests.
    fn get_basic_rate() -> QueriedExchangeRate {
        let base_asset = Asset {
            symbol: "ICP".to_string(),
            class: AssetClass::Cryptocurrency,
        };
        let quote_asset = Asset {
            symbol: USDT.to_string(),
            class: AssetClass::Cryptocurrency,
        };
        QueriedExchangeRate {
            base_asset,
            quote_asset,
            timestamp: 100,
            rates: vec![1_230_000],
            num_queried_sources: 1,
            num_received_rates: 1,
        }
    }

    /// The test verifies that the quote asset rate is always returned.
    #[test]
    fn verify_quote_asset_rate() {
        let mut cache = ExchangeRateCache::new(USDT.to_string(), 10, 20);
        let rate = cache.get(USDT, 12345, 678910);
        assert_eq!(cache.size(), 0);
        assert!(matches!(rate, Some(usdt_rate) if usdt_rate.rates == vec![10_000]));
    }

    /// The test verifies that entries are not cached if the quote asset does not match
    /// the quote asset defined for the cache.
    #[test]
    fn insert_rate_with_wrong_quote_asset() {
        let mut cache = ExchangeRateCache::new(USDT.to_string(), 10, 20);
        let mut basic_rate = get_basic_rate();
        basic_rate.quote_asset.symbol = USDC.to_string();
        assert!(matches!(
            cache.insert(basic_rate, 12345, 678910),
            Err(message) if message == *"Invalid quote asset: USDC".to_string()));
    }

    /// The test verifies that insertion works as expected.
    #[test]
    fn verify_cache_insert() {
        let retention_period_s = 60;

        let mut cache = ExchangeRateCache::new(USDT.to_string(), 10, 20);
        let basic_rate = get_basic_rate();

        cache
            .insert(basic_rate.clone(), 150, 150 + retention_period_s)
            .expect("Inserting should work.");
        assert_eq!(cache.size(), 1);

        // A rate is cached if the timestamp is different, even when inserting at the same time.
        let mut rate = basic_rate.clone();
        rate.timestamp = 120;
        cache
            .insert(rate, 150, 150 + retention_period_s)
            .expect("Inserting should work.");
        assert_eq!(cache.size(), 2);

        // Adding the first rate again at a different time replaces the first entry.
        cache
            .insert(basic_rate.clone(), 160, 160 + retention_period_s)
            .expect("Inserting should work.");
        assert_eq!(cache.size(), 2);
        let cached_rate = &cache.rates.get("ICP").unwrap()[1];
        assert_eq!(cached_rate.expiration_time, 160 + retention_period_s);
        assert_eq!(cached_rate.logical_time, 2);

        // At this point, the cache contains two records inserted at times 150 and 160, respectively.
        // When adding records 'expiration_time' and '2*expiration_time' later, the first two records
        // are evicted.
        let mut rate = basic_rate.clone();
        rate.timestamp = 210;
        cache
            .insert(rate, 210, 210 + retention_period_s)
            .expect("Inserting should work.");
        assert_eq!(cache.size(), 2);
        let cached_rate = &cache.rates.get("ICP").unwrap()[1];
        assert_eq!(cached_rate.expiration_time, 150 + 2 * retention_period_s);
        assert_eq!(cached_rate.logical_time, 3);

        // The second record is removed.
        let mut rate = basic_rate;
        rate.timestamp = 220;
        cache
            .insert(rate, 220, 220 + retention_period_s)
            .expect("Inserting should work.");
        assert_eq!(cache.size(), 2);
        let cached_rate = &cache.rates.get("ICP").unwrap()[1];
        assert_eq!(cached_rate.expiration_time, 160 + 2 * retention_period_s);
        assert_eq!(cached_rate.logical_time, 4);
    }

    /// The test verifies that getting cached exchange rates works as expected.
    #[test]
    fn verify_cache_get() {
        let retention_period_s = 60;
        let mut cache = ExchangeRateCache::new(USDT.to_string(), 10, 20);
        let basic_rate = get_basic_rate();
        cache
            .insert(basic_rate.clone(), 150, 150 + retention_period_s)
            .expect("Inserting should work.");
        assert!(matches!(cache.get("ICP", 100, 150), Some(_)));
        assert!(matches!(cache.get("ICP", 150, 150), None));
        assert!(matches!(cache.get("BTC", 100, 150), None));

        // A different cryptocurrency can be inserted and looked up as well.
        let mut btc_rate = basic_rate.clone();
        btc_rate.base_asset.symbol = "BTC".to_string();
        cache
            .insert(btc_rate, 160, 160 + retention_period_s)
            .expect("Inserting should work.");
        assert!(matches!(cache.get("BTC", 100, 160), Some(_)));

        // Insert another ICP rate at a later time.
        let mut icp_rate = basic_rate;
        icp_rate.timestamp = 190;
        cache
            .insert(icp_rate, 190, 190 + retention_period_s)
            .expect("Inserting should work.");
        assert_eq!(cache.size(), 3);

        // A look-up in the future only evicts the rates stored for the queried symbol.
        let rate_option = cache.get("ETH", 100, 1000);
        assert!(matches!(rate_option, None));
        assert_eq!(cache.size(), 3);

        // A look-up in the future for BTC removes the BTC rate.
        let rate_option = cache.get("BTC", 100, 1000);
        assert!(matches!(rate_option, None));
        assert_eq!(cache.size(), 2);

        // A look-up in the future for ICP removes the ICP rates.
        let rate_option = cache.get("ICP", 100, 150 + retention_period_s - 1);
        assert!(matches!(rate_option, Some(_)));
        assert_eq!(cache.size(), 2);
        let rate_option = cache.get("ICP", 100, 150 + retention_period_s);
        assert!(matches!(rate_option, None));
        assert_eq!(cache.size(), 1);
        let rate_option = cache.get("ICP", 190, 190 + retention_period_s - 1);
        assert!(matches!(rate_option, Some(_)));
        assert_eq!(cache.size(), 1);
        let rate_option = cache.get("ICP", 190, 190 + retention_period_s);
        assert!(matches!(rate_option, None));
        assert_eq!(cache.size(), 0);
    }

    /// The test verifies that the cache is pruned correctly when reaching the hard size limit.
    #[test]
    fn verify_cache_pruning() {
        let retention_period_s = 60;

        let mut cache = ExchangeRateCache::new(USDT.to_string(), 3, 5);
        let mut rate = get_basic_rate();
        cache
            .insert(rate.clone(), 100, 100 + retention_period_s)
            .expect("Inserting should work.");
        // Insert `hard_max_size = 5` rates, triggering the pruning.
        rate.base_asset.symbol = "ETH".to_string();
        cache
            .insert(rate.clone(), 100, 100 + retention_period_s)
            .expect("Inserting should work.");
        rate.base_asset.symbol = "BTC".to_string();
        cache
            .insert(rate.clone(), 100, 100 + retention_period_s)
            .expect("Inserting should work.");
        rate.base_asset.symbol = "ICP".to_string();
        rate.timestamp = 120;
        cache
            .insert(rate.clone(), 100, 100 + retention_period_s)
            .expect("Inserting should work.");
        // All rates should be cached.
        assert_eq!(cache.size(), 4);
        rate.timestamp = 140;
        cache
            .insert(rate.clone(), 100, 100 + retention_period_s)
            .expect("Inserting should work.");
        // The cache size should be reduced to `soft_max_size = 3`.
        assert_eq!(cache.size(), 3);
        assert!(matches!(cache.get("ETH", 100, 100), None));
        assert!(matches!(cache.get("BTC", 100, 100), Some(_)));

        // Insert more rates to trigger the pruning again.
        rate.base_asset.symbol = "ETH".to_string();
        cache
            .insert(rate.clone(), 100, 100 + retention_period_s)
            .expect("Inserting should work.");
        rate.timestamp = 160;
        cache
            .insert(rate, 100, 100 + retention_period_s)
            .expect("Inserting should work.");

        // The BTC rate is still there because it was accessed using `get`,
        // which increased its timestamp, whereas the ICP rates have been evicted.
        assert_eq!(cache.size(), 3);
        assert!(matches!(cache.get("ICP", 120, 100), None));
        assert!(matches!(cache.get("ICP", 140, 100), None));
        assert!(matches!(cache.get("BTC", 100, 100), Some(_)));
        assert!(matches!(cache.get("ETH", 140, 100), Some(_)));
        assert!(matches!(cache.get("ETH", 160, 100), Some(_)));
    }

    /// The test verifies that the cache respects different retention periods.
    #[test]
    fn verify_variable_retention_periods() {
        let mut cache = ExchangeRateCache::new(USDT.to_string(), 3, 5);
        let mut rate = get_basic_rate();
        cache
            .insert(rate.clone(), 100, 120)
            .expect("Inserting should work.");

        rate.timestamp = 110;
        cache
            .insert(rate.clone(), 110, 190)
            .expect("Inserting should work.");
        assert_eq!(cache.size(), 2);

        rate.timestamp = 120;
        // The first entry should be removed.
        cache
            .insert(rate.clone(), 120, 160)
            .expect("Inserting should work.");
        assert_eq!(cache.size(), 2);

        rate.timestamp = 150;
        cache
            .insert(rate.clone(), 150, 160)
            .expect("Inserting should work.");
        assert_eq!(cache.size(), 3);

        // Overriding the entry with timestamp = 110 does not increase the cache size.
        // The other entries have expired at time 188, reducing the cache size to 1.
        rate.timestamp = 110;
        cache
            .insert(rate, 188, 190)
            .expect("Inserting should work.");
        assert_eq!(cache.size(), 1);
    }
}
