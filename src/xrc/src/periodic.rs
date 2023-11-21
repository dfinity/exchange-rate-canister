use std::{cell::Cell, collections::HashSet};

use async_trait::async_trait;
use chrono::{Datelike, NaiveDateTime, Weekday};
use futures::future::join_all;

use crate::{
    call_forex,
    forex::{Forex, ForexContextArgs, ForexRateMap, FOREX_SOURCES},
    with_forex_rate_collector, with_forex_rate_collector_mut, with_forex_rate_store_mut,
    CallForexError, LOG_PREFIX, ONE_DAY_SECONDS, ONE_HOUR_SECONDS, USD,
};

thread_local! {
    static NEXT_RUN_SCHEDULED_AT_TIMESTAMP: Cell<u64> = Cell::new(0);
    static IS_UPDATING_FOREX_STORE: Cell<bool> = Cell::new(false);
}

// 6 hours in seconds
const SIX_HOURS: u64 = 6 * ONE_HOUR_SECONDS;

fn get_next_run_scheduled_at_timestamp() -> u64 {
    NEXT_RUN_SCHEDULED_AT_TIMESTAMP.with(|cell| cell.get())
}

fn set_next_run_scheduled_at_timestamp(timestamp: u64) {
    NEXT_RUN_SCHEDULED_AT_TIMESTAMP.with(|cell| cell.set(timestamp));
}

struct UpdatingForexStoreGuard;

impl UpdatingForexStoreGuard {
    fn new() -> Option<Self> {
        if IS_UPDATING_FOREX_STORE.with(|cell| cell.get()) {
            return None;
        }

        IS_UPDATING_FOREX_STORE.with(|cell| cell.set(true));
        Some(Self)
    }
}

impl Drop for UpdatingForexStoreGuard {
    fn drop(&mut self) {
        IS_UPDATING_FOREX_STORE.with(|cell| cell.set(false));
    }
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
        let forexes_with_times_and_context = get_forexes_with_timestamps_and_context(timestamp);
        // Extract the names, times and futures into separate lists
        let mut futures = vec![];
        let mut forex_names = vec![];
        let mut times = vec![];

        for (forex, timestamp, context) in forexes_with_times_and_context {
            forex_names.push(forex.to_string());
            times.push(timestamp);
            futures.push(call_forex(forex, context));
        }

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

#[derive(Debug, PartialEq, Eq)]
enum ForexStatusError {
    IpV4NotSupported,
    Weekend,
    AlreadyCollected,
}

fn check_forex_status(forex: &Forex, timestamp: u64) -> Result<(), ForexStatusError> {
    if !forex.is_available() {
        return Err(ForexStatusError::IpV4NotSupported);
    }

    // Avoid querying on weekends
    if !cfg!(feature = "disable-forex-weekend-check") {
        if let Weekday::Sat | Weekday::Sun =
            NaiveDateTime::from_timestamp(timestamp as i64, 0).weekday()
        {
            return Err(ForexStatusError::Weekend);
        }
    }

    if let Some(exclude) = with_forex_rate_collector(|c| c.get_sources(timestamp)) {
        // Avoid calling a source for which rates are already available for the requested date.
        if exclude.contains(&forex.to_string()) {
            return Err(ForexStatusError::AlreadyCollected);
        }
    }

    Ok(())
}

/// Helper function that builds out the timestamps and context args when
/// calling each forex.
fn get_forexes_with_timestamps_and_context(
    timestamp: u64,
) -> Vec<(&'static Forex, u64, ForexContextArgs)> {
    FOREX_SOURCES
        .iter()
        .filter_map(|forex| {
            // We always ask for the timestamp of yesterday's date, in the timezone of the source
            // This value will later be used as the key to update the collector.
            let key_timestamp = ((forex.offset_timestamp_to_timezone(timestamp) - ONE_DAY_SECONDS)
                / ONE_DAY_SECONDS)
                * ONE_DAY_SECONDS;

            if check_forex_status(forex, key_timestamp).is_err() {
                return None;
            }

            // We may need to shift the timestamp for querying.
            // For instance, CentralBankOfBosniaHerzegovina needs to shift +1 day to
            // get the rate at the `key_timestamp`.
            let query_timestamp = forex.offset_timestamp_for_query(key_timestamp);
            let context = ForexContextArgs {
                timestamp: query_timestamp,
            };

            Some((forex, key_timestamp, context))
        })
        .collect()
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

    let _guard = match UpdatingForexStoreGuard::new() {
        Some(guard) => guard,
        None => return UpdateForexStoreResult::AlreadyRunning,
    };

    let start_of_day = start_of_day_timestamp(timestamp);
    let (forex_rates, errors) = forex_sources.call(start_of_day).await;
    for (forex, error) in errors {
        ic_cdk::println!("{} {} {}", LOG_PREFIX, forex, error);
    }

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
        if let Some(mut forex_multi_rate_map) =
            with_forex_rate_collector(|collector| collector.get_rates_map(timestamp))
        {
            // Remove the USD rate from the rate map as all rates are assumed to be in USD.
            forex_multi_rate_map.remove(USD);
            with_forex_rate_store_mut(|store| store.put(timestamp, forex_multi_rate_map));
        }
    }

    set_next_run_scheduled_at_timestamp(get_next_run_timestamp(timestamp));
    UpdateForexStoreResult::Success
}

fn start_of_day_timestamp(timestamp: u64) -> u64 {
    timestamp - (timestamp % ONE_DAY_SECONDS)
}

fn get_next_run_timestamp(timestamp: u64) -> u64 {
    (timestamp - (timestamp % SIX_HOURS)) + SIX_HOURS
}

#[cfg(test)]
mod test {

    use futures::FutureExt;
    use maplit::hashmap;

    use crate::forex::COMPUTED_XDR_SYMBOL;
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
            COMPUTED_XDR_SYMBOL.to_string() => 10_000,
        };
        // The [ForexRatesStore] would only return a value if it has sufficiently many sources for CXDR.
        let mock_forex_sources =
            MockForexSourcesImpl::new(vec![map.clone(), map.clone(), map.clone(), map], vec![]);
        update_forex_store(timestamp, &mock_forex_sources)
            .now_or_never()
            .expect("should have executed");
        let result = with_forex_rate_store(|store| {
            store.get(start_of_day, timestamp + ONE_DAY_SECONDS, "eur", "usd")
        });
        assert!(
            matches!(result, Ok(ref forex_rate) if forex_rate.rates == vec![10_000, 10_000, 10_000, 10_000]),
            "Instead found {:#?}",
            result
        );
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
        IS_UPDATING_FOREX_STORE.with(|c| c.set(true));

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

    #[test]
    fn updating_forex_store_guard() {
        // Check state is initialized correctly.
        assert!(!IS_UPDATING_FOREX_STORE.with(|c| c.get()));
        // Create the guard and ensure the state has been update to true.
        let guard = UpdatingForexStoreGuard::new();
        assert!(IS_UPDATING_FOREX_STORE.with(|c| c.get()));

        // Drop the guard to reset the state.
        drop(guard);

        // Ensure the flag is reset.
        assert!(!IS_UPDATING_FOREX_STORE.with(|c| c.get()));
    }

    #[test]
    #[cfg(not(feature = "ipv4-support"))]
    fn check_forex_status_ipv4_not_supported() {
        let forex = FOREX_SOURCES.get(3).expect("ECB expected"); // European Central Bank
        assert!(matches!(
            check_forex_status(forex, 1680220800),
            Err(ForexStatusError::IpV4NotSupported)
        ));
    }

    #[test]
    fn check_forex_status_weekend() {
        let forex = FOREX_SOURCES.get(0).expect("Myanmar expected"); // Myanmar
        assert!(matches!(
            check_forex_status(forex, 1680372000),
            Err(ForexStatusError::Weekend)
        ));
    }

    #[test]
    fn check_forex_status_already_collected() {
        let timestamp = 1680220800;
        let forex = FOREX_SOURCES.get(0).expect("Myanmar expected"); // Myanmar
        with_forex_rate_collector_mut(|collector| {
            collector.update(
                forex.to_string(),
                timestamp,
                hashmap! {
                    "EUR".to_string() => 100
                },
            )
        });
        let result = check_forex_status(forex, timestamp);
        assert!(matches!(result, Err(ForexStatusError::AlreadyCollected)));
    }

    #[test]
    fn check_forex_status_is_ok() {
        let timestamp = 1680220800;
        let forex = FOREX_SOURCES.get(0).expect("Myanmar expected"); // Myanmar
        let result = check_forex_status(forex, timestamp);
        assert!(matches!(result, Ok(())));
    }

    #[test]
    #[cfg(not(feature = "ipv4-support"))]
    fn successfully_get_forexes_with_timestamps_and_context() {
        let timestamp = 1680220800;
        let forexes_with_timestamps_and_context =
            get_forexes_with_timestamps_and_context(timestamp);
        // Currently, 2. Once ipv4 flag is removed, 6.
        assert_eq!(forexes_with_timestamps_and_context.len(), 2);

        assert!(matches!(
            forexes_with_timestamps_and_context[0].0,
            Forex::CentralBankOfMyanmar(_)
        ));
        assert_eq!(forexes_with_timestamps_and_context[0].1, 1680134400);
        assert_eq!(
            forexes_with_timestamps_and_context[1].2.timestamp,
            1680220800
        );
        assert!(matches!(
            forexes_with_timestamps_and_context[1].0,
            Forex::CentralBankOfBosniaHerzegovina(_)
        ));
        assert_eq!(forexes_with_timestamps_and_context[1].1, 1680134400);
        assert_eq!(
            forexes_with_timestamps_and_context[1].2.timestamp,
            1680220800
        );
    }

    #[test]
    #[cfg(feature = "ipv4-support")]
    fn successfully_get_forexes_with_timestamps_and_context() {
        let timestamp = 1680220800;
        let forexes_with_timestamps_and_context =
            get_forexes_with_timestamps_and_context(timestamp);
        assert_eq!(forexes_with_timestamps_and_context.len(), 10);

        assert!(matches!(
            forexes_with_timestamps_and_context[0].0,
            Forex::CentralBankOfMyanmar(_)
        ));
        assert_eq!(forexes_with_timestamps_and_context[0].1, 1680134400);
        assert_eq!(
            forexes_with_timestamps_and_context[0].2.timestamp,
            1680134400
        );
        assert!(matches!(
            forexes_with_timestamps_and_context[1].0,
            Forex::CentralBankOfBosniaHerzegovina(_)
        ));
        assert_eq!(forexes_with_timestamps_and_context[1].1, 1680134400);
        assert_eq!(
            forexes_with_timestamps_and_context[1].2.timestamp,
            1680220800
        );
        assert!(matches!(
            forexes_with_timestamps_and_context[2].0,
            Forex::EuropeanCentralBank(_)
        ));
        assert_eq!(forexes_with_timestamps_and_context[2].1, 1680134400);
        assert_eq!(
            forexes_with_timestamps_and_context[2].2.timestamp,
            1680134400
        );

        assert!(matches!(
            forexes_with_timestamps_and_context[3].0,
            Forex::BankOfCanada(_)
        ));
        assert_eq!(forexes_with_timestamps_and_context[3].1, 1680048000);
        assert_eq!(
            forexes_with_timestamps_and_context[3].2.timestamp,
            1680048000
        );

        assert!(matches!(
            forexes_with_timestamps_and_context[4].0,
            Forex::CentralBankOfUzbekistan(_)
        ));
        assert_eq!(forexes_with_timestamps_and_context[4].1, 1680134400);
        assert_eq!(
            forexes_with_timestamps_and_context[4].2.timestamp,
            1680134400
        );

        assert!(matches!(
            forexes_with_timestamps_and_context[5].0,
            Forex::ReserveBankOfAustralia(_)
        ));
        assert_eq!(forexes_with_timestamps_and_context[5].1, 1680134400);
        assert_eq!(
            forexes_with_timestamps_and_context[5].2.timestamp,
            1680134400
        );

        assert!(matches!(
            forexes_with_timestamps_and_context[6].0,
            Forex::CentralBankOfNepal(_)
        ));
        assert_eq!(forexes_with_timestamps_and_context[6].1, 1680134400);
        assert_eq!(
            forexes_with_timestamps_and_context[6].2.timestamp,
            1680134400
        );

        assert!(matches!(
            forexes_with_timestamps_and_context[7].0,
            Forex::CentralBankOfGeorgia(_)
        ));
        assert_eq!(forexes_with_timestamps_and_context[7].1, 1680134400);
        assert_eq!(
            forexes_with_timestamps_and_context[7].2.timestamp,
            1680220800
        );

        assert!(matches!(
            forexes_with_timestamps_and_context[8].0,
            Forex::BankOfItaly(_)
        ));
        assert_eq!(forexes_with_timestamps_and_context[8].1, 1680134400);
        assert_eq!(
            forexes_with_timestamps_and_context[8].2.timestamp,
            1680134400
        );

        assert!(matches!(
            forexes_with_timestamps_and_context[9].0,
            Forex::SwissFederalOfficeForCustoms(_)
        ));
        assert_eq!(forexes_with_timestamps_and_context[9].1, 1680134400);
        assert_eq!(
            forexes_with_timestamps_and_context[9].2.timestamp,
            1680134400
        );
    }
}
