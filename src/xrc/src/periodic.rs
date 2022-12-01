use std::{cell::Cell, collections::HashSet};

use async_trait::async_trait;
use futures::future::join_all;

use crate::{
    call_forex,
    forex::{ForexContextArgs, ForexRateMap, FOREX_SOURCES},
    with_forex_rate_collector, with_forex_rate_collector_mut, with_forex_rate_store_mut,
    CallForexError,
};

thread_local! {
    static NEXT_RUN_SCHEDULED_AT_TIMESTAMP: Cell<u64> = Cell::new(0);
    static IS_UPDATING_FOREX_STORE: Cell<bool> = Cell::new(false);
}

// 1 minute in seconds
const ONE_MINUTE: u64 = 60;
// 1 hour in seconds
const ONE_HOUR: u64 = 60 * ONE_MINUTE;
// 6 hours in seconds
const SIX_HOURS: u64 = 6 * ONE_HOUR;
// 1 day in seconds
const ONE_DAY: u64 = 24 * ONE_HOUR;

fn get_next_run_scheduled_at_timestamp() -> u64 {
    NEXT_RUN_SCHEDULED_AT_TIMESTAMP.with(|cell| cell.get())
}

fn set_next_run_scheduled_at_timestamp(timestamp: u64) {
    NEXT_RUN_SCHEDULED_AT_TIMESTAMP.with(|cell| cell.set(timestamp));
}

fn is_updating_forex_store() -> bool {
    IS_UPDATING_FOREX_STORE.with(|cell| cell.get())
}

fn set_is_updating_forex_store(is_updating: bool) {
    IS_UPDATING_FOREX_STORE.with(|cell| cell.set(is_updating))
}

#[async_trait]
trait ForexSources {
    async fn call(
        &self,
        timestamp: u64,
    ) -> (
        Vec<(String, u64, ForexRateMap)>,
        Vec<(String, CallForexError)>,
    );
}

struct ForexSourcesImpl;

#[async_trait]
impl ForexSources for ForexSourcesImpl {
    async fn call(
        &self,
        timestamp: u64,
    ) -> (
        Vec<(String, u64, ForexRateMap)>,
        Vec<(String, CallForexError)>,
    ) {
        let futures_with_times = FOREX_SOURCES.iter().filter_map(|forex| {
            // We always ask for the timestamp of yesterday's date, in the timezone of the source
            let timestamp =
                ((forex.offset_timestamp_to_timezone(timestamp) - ONE_DAY) / ONE_DAY) * ONE_DAY;
            // But some sources expect an offset (e.g., today's date for yesterday's rate)
            let timestamp = forex.offset_timestamp_for_query(timestamp);
            if let Some(exclude) = with_forex_rate_collector(|c| c.get_sources(timestamp)) {
                // Avoid calling a source for which rates are already available for the requested date.
                if exclude.contains(&forex.to_string()) {
                    return None;
                }
            }
            if !cfg!(feature = "ipv4-support") && !forex.supports_ipv6() {
                return None;
            }

            Some((
                forex.to_string(),
                timestamp,
                call_forex(forex, ForexContextArgs { timestamp }),
            ))
        });
        // Extract the names, times and futures into separate lists
        let mut forex_names = vec![];
        let mut times = vec![];
        let futures = futures_with_times.map(|(name, timestamp, future)| {
            forex_names.push(name);
            times.push(timestamp);
            future
        });
        // Await all futures to complete
        let joined = join_all(futures).await;
        // Zip times and results
        let results = times.into_iter().zip(joined.into_iter());
        let mut rates = vec![];
        let mut errors = vec![];

        for (forex, (timestamp, result)) in forex_names.iter().zip(results) {
            match result {
                Ok(map) => {
                    if map.is_empty() {
                        errors.push((
                            forex.to_string(),
                            CallForexError::Http {
                                forex: forex.to_string(),
                                error: "Empty rates map".to_string(),
                            },
                        ))
                    } else {
                        rates.push((forex.to_string(), timestamp, map));
                    }
                }
                Err(error) => errors.push((forex.to_string(), error)),
            }
        }

        (rates, errors)
    }
}

/// Entrypoint for background tasks that need to be executed in the heartbeat.
pub async fn run_tasks(timestamp: u64) {
    let forex_sources = ForexSourcesImpl;
    update_forex_store(timestamp, &forex_sources).await;
}

#[derive(Debug)]
enum UpdateForexStoreResult {
    AlreadyRunning,
    NotReady,
    Success,
}

async fn update_forex_store(
    timestamp: u64,
    forex_sources: &impl ForexSources,
) -> UpdateForexStoreResult {
    let next_run_at = get_next_run_scheduled_at_timestamp();
    if next_run_at > 0 && next_run_at > timestamp {
        return UpdateForexStoreResult::NotReady;
    }

    if is_updating_forex_store() {
        return UpdateForexStoreResult::AlreadyRunning;
    }

    set_is_updating_forex_store(true);

    let start_of_day = start_of_day_timestamp(timestamp);
    let (forex_rates, _) = forex_sources.call(start_of_day).await;
    let mut timestamps_to_update: HashSet<u64> = HashSet::new();
    for (source, timestamp, rates) in forex_rates {
        // Try to update the collector with data from this source
        if with_forex_rate_collector_mut(|collector| collector.update(source, timestamp, rates)) {
            // Add timestamp to later update the forex store for the corresponding day
            timestamps_to_update.insert(timestamp);
        }
    }
    // Update the forex store with all days we collected new rates for
    for timestamp in timestamps_to_update {
        if let Some(forex_multi_rate_map) =
            with_forex_rate_collector(|collector| collector.get_rates_map(timestamp))
        {
            with_forex_rate_store_mut(|store| store.put(timestamp, forex_multi_rate_map));
        }
    }

    set_next_run_scheduled_at_timestamp(get_next_run_timestamp(timestamp));
    set_is_updating_forex_store(false);
    UpdateForexStoreResult::Success
}

fn start_of_day_timestamp(timestamp: u64) -> u64 {
    timestamp - (timestamp % ONE_DAY)
}

fn get_next_run_timestamp(timestamp: u64) -> u64 {
    (timestamp - (timestamp % SIX_HOURS)) + SIX_HOURS
}

#[cfg(test)]
mod test {

    use futures::FutureExt;
    use maplit::hashmap;

    use crate::with_forex_rate_store;

    use super::*;

    #[derive(Default)]
    struct MockForexSourcesImpl {
        maps: Vec<ForexRateMap>,
        errors: Vec<(String, CallForexError)>,
    }

    impl MockForexSourcesImpl {
        fn new(maps: Vec<ForexRateMap>, errors: Vec<(String, CallForexError)>) -> Self {
            Self { maps, errors }
        }
    }

    #[async_trait]
    impl ForexSources for MockForexSourcesImpl {
        async fn call(
            &self,
            timestamp: u64,
        ) -> (
            Vec<(String, u64, ForexRateMap)>,
            Vec<(String, CallForexError)>,
        ) {
            (
                self.maps
                    .clone()
                    .into_iter()
                    .map(|m| ("src_name".to_string(), timestamp, m))
                    .collect(),
                self.errors
                    .iter()
                    .map(|e| (e.0.clone(), e.1.clone()))
                    .collect(),
            )
        }
    }

    /// This function demonstrates that the forex rate store can be successfully updated by [update_forex_store].
    #[test]
    fn forex_store_can_be_updated_successfully() {
        let timestamp = 1666371931;
        let start_of_day = start_of_day_timestamp(timestamp);
        let map = hashmap! {
            "EUR".to_string() => 10_000,
            "SGD".to_string() => 1_000,
            "CHF".to_string() => 7_000,
        };
        let mock_forex_sources = MockForexSourcesImpl::new(vec![map], vec![]);
        update_forex_store(timestamp, &mock_forex_sources)
            .now_or_never()
            .expect("should have executed");
        let result =
            with_forex_rate_store(|store| store.get(start_of_day, timestamp, "eur", "usd"));
        assert!(matches!(result, Ok(forex_rate) if forex_rate.rates == vec![10_000]));
    }

    /// This function demonstrates that the forex rate store can be successfully updated by [update_forex_store]
    /// on a six hour interval controlled by the [NEXT_RUN_SCHEDULED_AT_TIMESTAMP] state variable.
    #[test]
    fn forex_store_can_be_updated_on_six_hour_interval() {
        let mock_forex_sources = MockForexSourcesImpl::default();
        set_next_run_scheduled_at_timestamp(1666375200);

        let timestamp = 1666371931;
        let result = update_forex_store(timestamp, &mock_forex_sources)
            .now_or_never()
            .expect("should complete");

        assert!(matches!(result, UpdateForexStoreResult::NotReady));

        let timestamp = 1666375201;
        let result = update_forex_store(timestamp, &mock_forex_sources)
            .now_or_never()
            .expect("should complete");

        assert!(matches!(result, UpdateForexStoreResult::Success));
        let next_timestamp = get_next_run_scheduled_at_timestamp();
        assert_eq!(next_timestamp, 1666396800);
    }

    /// This function demonstrates that [update_forex_store] should exit early if there is already
    /// an instance running indicated by the [IS_UPDATING_FOREX_STORE] state variable.
    #[test]
    fn forex_store_can_only_be_updated_once_at_a_time() {
        set_is_updating_forex_store(true);

        let timestamp = 1666371931;
        let mock_forex_sources = MockForexSourcesImpl::default();
        let result = update_forex_store(timestamp, &mock_forex_sources)
            .now_or_never()
            .expect("should complete");
        assert!(matches!(result, UpdateForexStoreResult::AlreadyRunning));
    }

    #[test]
    fn start_of_the_day_timestamp_can_be_derived() {
        // Friday, October 21, 2022 17:05:31 UTC
        let timestamp = 1666371931;
        // Friday, October 21, 2022 0:00:00 UTC
        assert_eq!(start_of_day_timestamp(timestamp), 1666310400);
    }

    #[test]
    fn get_next_run_timestamp_can_be_derived() {
        // Friday, October 21, 2022 17:05:31 UTC
        let timestamp = 1666371931;
        // Friday, October 21, 2022 18:00:00 UTC
        assert_eq!(get_next_run_timestamp(timestamp), 1666375200);

        let timestamp = 1666375201;
        // Saturday, October 22, 2022 0:00:00 UTC
        assert_eq!(get_next_run_timestamp(timestamp), 1666396800);

        let timestamp = 1666396801;
        // Saturday, October 22, 2022 6:00:00 UTC
        assert_eq!(get_next_run_timestamp(timestamp), 1666418400);

        let timestamp = 1666418401;
        // Saturday, October 22, 2022 12:00:00 UTC
        assert_eq!(get_next_run_timestamp(timestamp), 1666440000);
    }
}
