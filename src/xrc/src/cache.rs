
const QUOTE_ASSET : &str = "USDT";

struct ExchangeRateCache {
    capacity: usize,
    lru_cache: LruCache<(String, u64), QueriedExchangeRate>,
}
impl ExchangeRateCache {
    fn new(capacity: usize) -> Self {
        ExchangeRateCache {
            capacity,
            lru_cache: LruCache::new(NonZeroUsize::new(MAX_CACHE_SIZE).unwrap()),
        }
    }

    fn insert(&self, symbol: &str, timestamp: u64, rate: &QueriedExchangeRate) {
        if symbol != QUOTE_ASSET {
            self.lru_cache.put((rate.base_asset.symbol.clone(), timestamp), rate.clone());
        }
    }

    fn get(&self, symbol: &str, timestamp: u64) -> QueriedExchangeRate {
        if symbol == QUOTE_ASSET {
            QueriedExchangeRate::new()
        }
    }
}

