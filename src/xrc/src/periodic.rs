use std::cell::Cell;

use async_trait::async_trait;
use futures::future::join_all;

use crate::{
    call_forex,
    forex::{collect_rates, ForexContextArgs, ForexRateMap, FOREX_SOURCES},
    with_forex_rate_store_mut, CallForexError,
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
    async fn call(&self, timestamp: u64) -> (Vec<ForexRateMap>, Vec<(String, CallForexError)>);
}

struct ForexSourcesImpl;

#[async_trait]
impl ForexSources for ForexSourcesImpl {
    async fn call(&self, timestamp: u64) -> (Vec<ForexRateMap>, Vec<(String, CallForexError)>) {
        let args = ForexContextArgs { timestamp };
        let results = join_all(FOREX_SOURCES.iter().map(|forex| call_forex(forex, &args))).await;
        let mut rates = vec![];
        let mut errors = vec![];

        for (forex, result) in FOREX_SOURCES.iter().zip(results) {
            match result {
                Ok(map) => rates.push(map),
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
    // TODO: may need a method to seed the next run value on initialization
    if next_run_at > 0 && next_run_at > timestamp {
        return UpdateForexStoreResult::NotReady;
    }

    if is_updating_forex_store() {
        return UpdateForexStoreResult::AlreadyRunning;
    }

    set_is_updating_forex_store(true);

    // TODO: track errors using a counter for each forex
    let start_of_day = start_of_day_timestamp(timestamp);
    // TODO: what should happen if the forex_rate_maps vector is empty?
    let (forex_rate_maps, _) = forex_sources.call(start_of_day).await;
    let forex_multi_rate_map = collect_rates(start_of_day, forex_rate_maps);
    with_forex_rate_store_mut(|store| store.put(start_of_day, forex_multi_rate_map));

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
        async fn call(&self, _: u64) -> (Vec<ForexRateMap>, Vec<(String, CallForexError)>) {
            (
                self.maps.clone(),
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
            "eur".to_string() => 10_000,
            "sgd".to_string() => 1_000,
            "chf".to_string() => 7_000,
        };
        let mock_forex_sources = MockForexSourcesImpl::new(vec![map], vec![]);
        update_forex_store(timestamp, &mock_forex_sources)
            .now_or_never()
            .expect("should have executed");
        let result = with_forex_rate_store(|store| store.get(start_of_day, "eur", "usd"));
        assert!(matches!(result, Ok(forex_rate) if forex_rate.rate == 10_000));
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
