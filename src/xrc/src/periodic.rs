use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashSet},
};

use async_trait::async_trait;
use chrono::{DateTime, Datelike, Weekday};
use futures::future::join_all;

use crate::{
    call_exchange_listing, call_forex,
    exchanges::ListedPairs,
    forex::{Forex, ForexContextArgs, ForexRateMap, FOREX_SOURCES},
    increment_labeled_counter,
    listings::AcceptOutcome,
    set_labeled_gauge, with_forex_rate_collector, with_forex_rate_collector_mut,
    with_forex_rate_store_mut, with_listing_store, with_listing_store_mut, CallExchangeError,
    CallForexError, LabelKey, MetricName, Outcome, EXCHANGES, LOG_PREFIX, ONE_DAY_SECONDS,
    ONE_HOUR_SECONDS, USD,
};

thread_local! {
    static NEXT_RUN_SCHEDULED_AT_TIMESTAMP: Cell<u64> = const { Cell::new(0) };
    static IS_UPDATING_FOREX_STORE: Cell<bool> = const { Cell::new(false) };
    // Each exchange's listing is refreshed on its own schedule (see
    // `update_listing_store`): the next-attempt timestamp per exchange. A
    // missing entry means "due now" (treated as 0), so a fresh canister or the
    // first heartbeat after an upgrade sweeps every exchange once. Exactly one
    // timestamp per exchange, always overwritten — never a second queued slot —
    // so a long outage yields one outcall per retry interval, never a pile-up.
    static NEXT_LISTING_RUN_BY_EXCHANGE: RefCell<BTreeMap<String, u64>> =
        const { RefCell::new(BTreeMap::new()) };
    static IS_UPDATING_LISTING_STORE: Cell<bool> = const { Cell::new(false) };
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
        let results = times.into_iter().zip(joined);
        let mut rates = vec![];
        let mut errors = vec![];

        for (forex, (timestamp, result)) in forex_names.iter().zip(results) {
            match result {
                Ok(map) => {
                    if map.is_empty() {
                        errors.push((
                            forex.to_string(),
                            CallForexError::Empty {
                                forex: forex.to_string(),
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
        if let Weekday::Sat | Weekday::Sun = DateTime::from_timestamp(timestamp as i64, 0)
            .map(|t| t.weekday())
            .unwrap_or(Weekday::Mon)
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

    let listing_sources = ListingSourcesImpl;
    update_listing_store(timestamp, &listing_sources).await;
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

    record_per_forex_metrics(timestamp, &forex_rates, &errors);

    for (forex, error) in &errors {
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

    set_labeled_gauge(
        MetricName::PeriodicForexRunLastSeconds,
        &[],
        timestamp as f64,
    );

    set_next_run_scheduled_at_timestamp(get_next_run_timestamp(timestamp));
    UpdateForexStoreResult::Success
}

fn start_of_day_timestamp(timestamp: u64) -> u64 {
    timestamp - (timestamp % ONE_DAY_SECONDS)
}

fn record_per_forex_metrics(
    now_secs: u64,
    forex_rates: &[(String, u64, ForexRateMap)],
    errors: &[(String, CallForexError)],
) {
    for (forex, _, _) in forex_rates {
        increment_labeled_counter(
            MetricName::ForexFetchTotal,
            &[
                (LabelKey::Forex, forex.as_str()),
                (LabelKey::Outcome, Outcome::Success.into()),
            ],
        );
        set_labeled_gauge(
            MetricName::ForexLastSuccessSeconds,
            &[(LabelKey::Forex, forex.as_str())],
            now_secs as f64,
        );
    }
    for (forex, error) in errors {
        let outcome = match error {
            CallForexError::Empty { .. } => Outcome::EmptyMap,
            CallForexError::Http { .. } => Outcome::HttpError,
            CallForexError::Candid { .. } => Outcome::CandidError,
        };
        increment_labeled_counter(
            MetricName::ForexFetchTotal,
            &[
                (LabelKey::Forex, forex.as_str()),
                (LabelKey::Outcome, outcome.into()),
            ],
        );
    }
}

fn get_next_run_timestamp(timestamp: u64) -> u64 {
    (timestamp - (timestamp % SIX_HOURS)) + SIX_HOURS
}

// Each exchange's listing is refreshed roughly once a day.
const LISTING_REFRESH_INTERVAL: u64 = ONE_DAY_SECONDS;

// When an exchange's fetch fails or is rejected, retry it after this much
// shorter interval instead of waiting a full day, so a transient outage of one
// exchange doesn't cost a day of staleness for that exchange.
const LISTING_RETRY_INTERVAL: u64 = ONE_HOUR_SECONDS;

fn listing_next_attempt_at(exchange: &str) -> u64 {
    NEXT_LISTING_RUN_BY_EXCHANGE.with(|map| map.borrow().get(exchange).copied().unwrap_or(0))
}

fn set_listing_next_attempt_at(exchange: &str, timestamp: u64) {
    NEXT_LISTING_RUN_BY_EXCHANGE.with(|map| {
        map.borrow_mut().insert(exchange.to_string(), timestamp);
    });
}

// The aligned daily boundary strictly after `timestamp`: healthy exchanges all
// converge to the same daily refresh time regardless of when each last
// succeeded.
fn get_next_listing_run_timestamp(timestamp: u64) -> u64 {
    (timestamp - (timestamp % LISTING_REFRESH_INTERVAL)) + LISTING_REFRESH_INTERVAL
}

struct UpdatingListingStoreGuard;

impl UpdatingListingStoreGuard {
    fn new() -> Option<Self> {
        if IS_UPDATING_LISTING_STORE.with(|cell| cell.get()) {
            return None;
        }

        IS_UPDATING_LISTING_STORE.with(|cell| cell.set(true));
        Some(Self)
    }
}

impl Drop for UpdatingListingStoreGuard {
    fn drop(&mut self) {
        IS_UPDATING_LISTING_STORE.with(|cell| cell.set(false));
    }
}

/// Fetches per-exchange spot listings. Behind a trait so the refresh logic can
/// be unit-tested without real HTTP outcalls.
#[async_trait]
trait ListingSources {
    /// All exchanges this source can fetch — the candidates the scheduler picks
    /// the due ones from.
    fn exchange_names(&self) -> Vec<String>;

    /// One listing fetch per requested exchange: `(exchange name, result)`.
    async fn call(&self, exchanges: &[String])
        -> Vec<(String, Result<ListedPairs, CallExchangeError>)>;
}

struct ListingSourcesImpl;

#[async_trait]
impl ListingSources for ListingSourcesImpl {
    fn exchange_names(&self) -> Vec<String> {
        EXCHANGES
            .iter()
            .filter(|exchange| exchange.is_available())
            .map(|exchange| exchange.name().to_string())
            .collect()
    }

    async fn call(
        &self,
        exchanges: &[String],
    ) -> Vec<(String, Result<ListedPairs, CallExchangeError>)> {
        let mut names = vec![];
        let mut futures = vec![];
        for exchange in EXCHANGES
            .iter()
            .filter(|exchange| exchange.is_available() && exchanges.iter().any(|e| e == exchange.name()))
        {
            names.push(exchange.name().to_string());
            futures.push(call_exchange_listing(exchange));
        }
        names.into_iter().zip(join_all(futures).await).collect()
    }
}

#[derive(Debug, PartialEq, Eq)]
enum UpdateListingStoreResult {
    AlreadyRunning,
    NotReady,
    Success,
}

/// Refreshes the listing store for every exchange whose own next-attempt time
/// has elapsed. Each exchange is scheduled independently: an accepted listing
/// replaces the stored one and reschedules that exchange at the next daily
/// boundary, while a rejected (guard) or failed (HTTP) fetch leaves the
/// last-known-good listing in place and reschedules that exchange after the
/// short retry interval — so one exchange's outage is retried hourly without
/// dragging the healthy ones off their daily cadence.
async fn update_listing_store(
    timestamp: u64,
    listing_sources: &impl ListingSources,
) -> UpdateListingStoreResult {
    let _guard = match UpdatingListingStoreGuard::new() {
        Some(guard) => guard,
        None => return UpdateListingStoreResult::AlreadyRunning,
    };

    // Pick the exchanges due this tick (next-attempt time elapsed, or never
    // scheduled). Nothing due is the common case on most heartbeats.
    let due: Vec<String> = listing_sources
        .exchange_names()
        .into_iter()
        .filter(|exchange| listing_next_attempt_at(exchange) <= timestamp)
        .collect();
    if due.is_empty() {
        return UpdateListingStoreResult::NotReady;
    }

    // Reschedule each due exchange at the retry interval BEFORE the outcalls.
    // State changes made after an await are rolled back if the logic below
    // traps; without this a due exchange would re-fire on every heartbeat. As
    // each exchange holds exactly one next-attempt timestamp that we overwrite
    // (never a second queued slot), this also guarantees a long outage fires at
    // most one outcall per retry interval — never two-at-once after a day down.
    // Accepted exchanges are bumped to the daily boundary once the fetch lands.
    for exchange in &due {
        set_listing_next_attempt_at(exchange, timestamp + LISTING_RETRY_INTERVAL);
    }

    for (exchange, result) in listing_sources.call(&due).await {
        match result {
            Ok(listed) => {
                let outcome =
                    with_listing_store_mut(|store| store.accept(&exchange, listed, timestamp));
                match outcome {
                    AcceptOutcome::Accepted => {
                        // Success: rejoin the aligned daily cadence.
                        set_listing_next_attempt_at(
                            &exchange,
                            get_next_listing_run_timestamp(timestamp),
                        );
                        record_accepted_listing_metrics(&exchange);
                    }
                    rejected => {
                        // Keep the retry interval scheduled above.
                        increment_labeled_counter(
                            MetricName::ExchangeListingRejectedTotal,
                            &[(LabelKey::Exchange, &exchange), (LabelKey::Reason, "guard")],
                        );
                        ic_cdk::println!(
                            "{} [listing] {} refresh rejected: {:?}",
                            LOG_PREFIX,
                            exchange,
                            rejected
                        );
                    }
                }
            }
            Err(error) => {
                // Keep the last-known-good listing and the retry interval
                // scheduled above on a failed fetch. Log the structured error
                // (Debug): the Display impl is worded for rate fetching
                // ("Failed to retrieve rate from ..."), which would be
                // misleading for a listing refresh.
                increment_labeled_counter(
                    MetricName::ExchangeListingRejectedTotal,
                    &[(LabelKey::Exchange, &exchange), (LabelKey::Reason, "fetch")],
                );
                ic_cdk::println!(
                    "{} [listing] {} refresh fetch failed: {:?}",
                    LOG_PREFIX,
                    exchange,
                    error
                );
            }
        }
    }

    UpdateListingStoreResult::Success
}

/// Sets the three per-exchange listing gauges from the just-accepted listing.
fn record_accepted_listing_metrics(exchange: &str) {
    with_listing_store(|store| {
        if let Some(listing) = store.get(exchange) {
            set_labeled_gauge(
                MetricName::ExchangeListedUsdtPairs,
                &[(LabelKey::Exchange, exchange)],
                listing.bases.len() as f64,
            );
            set_labeled_gauge(
                MetricName::ExchangeListingTotalMarkets,
                &[(LabelKey::Exchange, exchange)],
                listing.total_markets as f64,
            );
            set_labeled_gauge(
                MetricName::ExchangeListingLastSuccessSeconds,
                &[(LabelKey::Exchange, exchange)],
                listing.last_success_secs as f64,
            );
        }
    });
}

#[cfg(test)]
mod test {

    use futures::FutureExt;
    use maplit::btreemap;

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
        crate::reset_labeled_metrics_for_test();
        let timestamp = 1666371931;
        let start_of_day = start_of_day_timestamp(timestamp);
        let map = btreemap! {
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
        crate::reset_labeled_metrics_for_test();
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
        let forex = FOREX_SOURCES.first().expect("Myanmar expected"); // Myanmar
        assert!(matches!(
            check_forex_status(forex, 1680372000),
            Err(ForexStatusError::Weekend)
        ));
    }

    #[test]
    fn check_forex_status_already_collected() {
        let timestamp = 1680220800;
        let forex = FOREX_SOURCES.first().expect("Myanmar expected"); // Myanmar
        with_forex_rate_collector_mut(|collector| {
            collector.update(
                forex.to_string(),
                timestamp,
                btreemap! {
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
        let forex = FOREX_SOURCES.first().expect("Myanmar expected"); // Myanmar
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
        assert_eq!(forexes_with_timestamps_and_context.len(), 11);

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

    mod listing_refresh {
        use super::*;
        use std::collections::BTreeSet;
        use std::sync::Mutex;

        /// A canned listing fetch outcome for one exchange.
        enum MockResult {
            Listed(ListedPairs),
            HttpError,
        }

        struct MockListingSources {
            results: Vec<(String, MockResult)>,
            /// The `due` list passed to each `call`, in order — so tests can
            /// assert exactly which exchanges were fetched on each tick.
            /// `Mutex` (not `RefCell`) so the boxed future stays `Send`.
            fetched: Mutex<Vec<Vec<String>>>,
        }

        impl MockListingSources {
            fn new(results: Vec<(String, MockResult)>) -> Self {
                Self {
                    results,
                    fetched: Mutex::new(vec![]),
                }
            }

            /// The `due` lists recorded across every `call`.
            fn fetched(&self) -> Vec<Vec<String>> {
                self.fetched.lock().unwrap().clone()
            }
        }

        #[async_trait]
        impl ListingSources for MockListingSources {
            fn exchange_names(&self) -> Vec<String> {
                self.results.iter().map(|(name, _)| name.clone()).collect()
            }

            async fn call(
                &self,
                exchanges: &[String],
            ) -> Vec<(String, Result<ListedPairs, CallExchangeError>)> {
                self.fetched.lock().unwrap().push(exchanges.to_vec());
                self.results
                    .iter()
                    .filter(|(name, _)| exchanges.iter().any(|e| e == name))
                    .map(|(name, result)| {
                        let result = match result {
                            MockResult::Listed(listed) => Ok(listed.clone()),
                            MockResult::HttpError => Err(CallExchangeError::Http {
                                exchange: name.clone(),
                                error: "boom".to_string(),
                            }),
                        };
                        (name.clone(), result)
                    })
                    .collect()
            }
        }

        fn listed(bases: &[&str], total_markets: usize) -> ListedPairs {
            ListedPairs {
                bases: bases.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>(),
                total_markets,
            }
        }

        fn base_set(bases: &[&str]) -> BTreeSet<String> {
            bases.iter().map(|s| s.to_string()).collect()
        }

        /// Reset every exchange's schedule so all are due to run.
        fn ready() {
            NEXT_LISTING_RUN_BY_EXCHANGE.with(|map| map.borrow_mut().clear());
        }

        /// An accepted refresh populates the store with the fetched bases.
        #[test]
        fn accepted_listing_populates_store() {
            ready();
            let sources = MockListingSources::new(vec![(
                "ListingTestAccept".to_string(),
                MockResult::Listed(listed(&["BTC", "ICP"], 300)),
            )]);
            let result = update_listing_store(1_000, &sources)
                .now_or_never()
                .expect("should complete");
            assert_eq!(result, UpdateListingStoreResult::Success);

            let stored = with_listing_store_mut(|store| store.get("ListingTestAccept").cloned())
                .expect("listing should be stored");
            assert_eq!(stored.bases, base_set(&["BTC", "ICP"]));
            assert_eq!(stored.total_markets, 300);
        }

        /// A failed (HTTP) fetch leaves the last-known-good listing untouched.
        #[test]
        fn failed_fetch_keeps_last_good() {
            ready();
            with_listing_store_mut(|store| store.accept("ListingTestErr", listed(&["BTC"], 500), 1));

            let sources =
                MockListingSources::new(vec![("ListingTestErr".to_string(), MockResult::HttpError)]);
            update_listing_store(2_000, &sources)
                .now_or_never()
                .expect("should complete");

            let stored = with_listing_store_mut(|store| store.get("ListingTestErr").cloned())
                .expect("listing should still be stored");
            assert_eq!(stored.total_markets, 500);
            assert_eq!(stored.last_success_secs, 1);
        }

        /// A refresh rejected by the structural guard keeps the last-good listing.
        #[test]
        fn rejected_refresh_keeps_last_good() {
            ready();
            with_listing_store_mut(|store| store.accept("ListingTestRej", listed(&["BTC"], 500), 1));

            // Below the absolute floor -> rejected.
            let sources = MockListingSources::new(vec![(
                "ListingTestRej".to_string(),
                MockResult::Listed(listed(&["BTC"], 3)),
            )]);
            update_listing_store(2_000, &sources)
                .now_or_never()
                .expect("should complete");

            let stored = with_listing_store_mut(|store| store.get("ListingTestRej").cloned())
                .expect("listing should still be stored");
            assert_eq!(stored.total_markets, 500);
        }

        /// An exchange is not fetched until its own next-attempt time has
        /// elapsed, and an accepted refresh reschedules it at the following
        /// daily boundary.
        #[test]
        fn respects_daily_interval() {
            ready();
            set_listing_next_attempt_at("ListingTestDaily", 1_000_000);
            let sources = MockListingSources::new(vec![(
                "ListingTestDaily".to_string(),
                MockResult::Listed(listed(&["BTC", "ETH"], 300)),
            )]);

            let result = update_listing_store(999_999, &sources)
                .now_or_never()
                .expect("should complete");
            assert_eq!(result, UpdateListingStoreResult::NotReady);

            let result = update_listing_store(1_000_001, &sources)
                .now_or_never()
                .expect("should complete");
            assert_eq!(result, UpdateListingStoreResult::Success);
            assert_eq!(
                listing_next_attempt_at("ListingTestDaily"),
                get_next_listing_run_timestamp(1_000_001)
            );
        }

        /// A failed fetch reschedules that exchange after the short retry
        /// interval rather than at the next daily boundary.
        #[test]
        fn failed_fetch_schedules_short_retry() {
            ready();
            let sources = MockListingSources::new(vec![(
                "ListingTestRetry".to_string(),
                MockResult::HttpError,
            )]);

            let result = update_listing_store(1_000, &sources)
                .now_or_never()
                .expect("should complete");
            assert_eq!(result, UpdateListingStoreResult::Success);
            assert_eq!(
                listing_next_attempt_at("ListingTestRetry"),
                1_000 + LISTING_RETRY_INTERVAL
            );
        }

        /// Each exchange is scheduled independently: a single exchange's outage
        /// is retried hourly while the healthy ones keep their daily cadence,
        /// and the down exchange snaps back to daily once it recovers.
        #[test]
        fn down_exchange_retries_hourly_without_dragging_healthy_ones() {
            ready();
            let down = "ListingTestDown".to_string();
            let healthy = "ListingTestHealthy".to_string();

            // First sweep: both due, one fails, one succeeds.
            let sources = MockListingSources::new(vec![
                (down.clone(), MockResult::HttpError),
                (healthy.clone(), MockResult::Listed(listed(&["BTC"], 300))),
            ]);
            update_listing_store(1_000, &sources)
                .now_or_never()
                .expect("should complete");
            assert_eq!(listing_next_attempt_at(&down), 1_000 + LISTING_RETRY_INTERVAL);
            assert_eq!(
                listing_next_attempt_at(&healthy),
                get_next_listing_run_timestamp(1_000)
            );

            // One retry interval later only the down exchange is due, and it
            // now recovers and rejoins the daily cadence.
            let later = 1_000 + LISTING_RETRY_INTERVAL;
            let sources = MockListingSources::new(vec![
                (down.clone(), MockResult::Listed(listed(&["ETH"], 300))),
                (healthy.clone(), MockResult::Listed(listed(&["BTC"], 300))),
            ]);
            update_listing_store(later, &sources)
                .now_or_never()
                .expect("should complete");
            // Only the down exchange was fetched this tick.
            assert_eq!(sources.fetched(), vec![vec![down.clone()]]);
            assert_eq!(
                listing_next_attempt_at(&down),
                get_next_listing_run_timestamp(later)
            );
        }

        /// A persistently failing exchange fires at most one outcall per retry
        /// interval — its single next-attempt timestamp is overwritten, never
        /// queued, so failures can't pile up into multiple simultaneous calls.
        #[test]
        fn persistent_failure_fires_once_per_interval_no_pileup() {
            ready();
            let exchange = "ListingTestNoPileup".to_string();
            let sources =
                MockListingSources::new(vec![(exchange.clone(), MockResult::HttpError)]);

            // Due now -> fires, reschedules one interval out.
            update_listing_store(1_000, &sources)
                .now_or_never()
                .expect("should complete");
            assert_eq!(listing_next_attempt_at(&exchange), 1_000 + LISTING_RETRY_INTERVAL);

            // Still within the interval -> not due, no fetch.
            let result = update_listing_store(1_000 + LISTING_RETRY_INTERVAL - 1, &sources)
                .now_or_never()
                .expect("should complete");
            assert_eq!(result, UpdateListingStoreResult::NotReady);

            // Interval elapsed -> fires once more, advancing by exactly one
            // interval (not two).
            update_listing_store(1_000 + LISTING_RETRY_INTERVAL, &sources)
                .now_or_never()
                .expect("should complete");
            assert_eq!(
                listing_next_attempt_at(&exchange),
                1_000 + 2 * LISTING_RETRY_INTERVAL
            );

            // Exactly one fetch per due tick, never a pile-up.
            assert_eq!(
                sources.fetched(),
                vec![vec![exchange.clone()], vec![exchange.clone()]]
            );
        }

        /// Only one refresh runs at a time.
        #[test]
        fn only_one_update_at_a_time() {
            ready();
            IS_UPDATING_LISTING_STORE.with(|cell| cell.set(true));
            let sources = MockListingSources::new(vec![]);

            let result = update_listing_store(1, &sources)
                .now_or_never()
                .expect("should complete");
            assert_eq!(result, UpdateListingStoreResult::AlreadyRunning);

            // Reset so the flag doesn't leak to another test on this thread.
            IS_UPDATING_LISTING_STORE.with(|cell| cell.set(false));
        }

        /// An accepted refresh sets the three per-exchange gauges; a rejected
        /// refresh increments the rejected counter under a `reason` label that
        /// distinguishes an HTTP failure (`fetch`) from a guard rejection
        /// (`guard`).
        #[test]
        fn records_gauges_on_accept_and_counter_on_failure() {
            use crate::{
                make_metric_key, reset_labeled_metrics_for_test, with_labeled_counters,
                with_labeled_gauges,
            };
            reset_labeled_metrics_for_test();
            ready();

            // Seed a last-known-good listing so the undersized refresh below is
            // rejected by the structural guard rather than accepted.
            with_listing_store_mut(|store| {
                store.accept("ListingTestMetricsGuard", listed(&["BTC"], 500), 1)
            });

            let sources = MockListingSources::new(vec![
                (
                    "ListingTestMetrics".to_string(),
                    MockResult::Listed(listed(&["BTC", "ICP"], 300)),
                ),
                ("ListingTestMetricsErr".to_string(), MockResult::HttpError),
                (
                    "ListingTestMetricsGuard".to_string(),
                    MockResult::Listed(listed(&["BTC"], 3)),
                ),
            ]);
            update_listing_store(1_700, &sources)
                .now_or_never()
                .expect("should complete");

            with_labeled_gauges(|m| {
                let pairs = make_metric_key(
                    MetricName::ExchangeListedUsdtPairs,
                    &[(LabelKey::Exchange, "ListingTestMetrics")],
                );
                assert_eq!(m.get(&pairs).copied(), Some(2.0));
                let total = make_metric_key(
                    MetricName::ExchangeListingTotalMarkets,
                    &[(LabelKey::Exchange, "ListingTestMetrics")],
                );
                assert_eq!(m.get(&total).copied(), Some(300.0));
                let last = make_metric_key(
                    MetricName::ExchangeListingLastSuccessSeconds,
                    &[(LabelKey::Exchange, "ListingTestMetrics")],
                );
                assert_eq!(m.get(&last).copied(), Some(1_700.0));
            });
            with_labeled_counters(|m| {
                let fetch_failed = make_metric_key(
                    MetricName::ExchangeListingRejectedTotal,
                    &[
                        (LabelKey::Exchange, "ListingTestMetricsErr"),
                        (LabelKey::Reason, "fetch"),
                    ],
                );
                assert_eq!(m.get(&fetch_failed).copied(), Some(1));
                let guard_rejected = make_metric_key(
                    MetricName::ExchangeListingRejectedTotal,
                    &[
                        (LabelKey::Exchange, "ListingTestMetricsGuard"),
                        (LabelKey::Reason, "guard"),
                    ],
                );
                assert_eq!(m.get(&guard_rejected).copied(), Some(1));
            });
        }
    }

    mod per_forex_metrics {
        use super::*;
        use crate::{
            make_metric_key, reset_labeled_metrics_for_test, with_labeled_counters,
            with_labeled_gauges,
        };

        fn reset() {
            reset_labeled_metrics_for_test();
        }

        #[test]
        fn success_increments_counter_and_sets_last_success() {
            reset();
            let now = 1_700_000_000_u64;
            let rates = vec![(
                "EuropeanCentralBank".to_string(),
                123,
                btreemap! { "EUR".to_string() => 10_000 },
            )];
            record_per_forex_metrics(now, &rates, &[]);

            with_labeled_counters(|m| {
                let key = make_metric_key(
                    MetricName::ForexFetchTotal,
                    &[
                        (LabelKey::Forex, "EuropeanCentralBank"),
                        (LabelKey::Outcome, Outcome::Success.into()),
                    ],
                );
                assert_eq!(m.get(&key).copied(), Some(1));
            });
            with_labeled_gauges(|m| {
                let key = make_metric_key(
                    MetricName::ForexLastSuccessSeconds,
                    &[(LabelKey::Forex, "EuropeanCentralBank")],
                );
                assert_eq!(m.get(&key).copied(), Some(now as f64));
            });
        }

        #[test]
        fn empty_map_error_is_recorded_as_empty_map_outcome() {
            reset();
            let errors = vec![(
                "BankOfCanada".to_string(),
                CallForexError::Empty {
                    forex: "BankOfCanada".to_string(),
                },
            )];
            record_per_forex_metrics(0, &[], &errors);

            with_labeled_counters(|m| {
                let key = make_metric_key(
                    MetricName::ForexFetchTotal,
                    &[
                        (LabelKey::Forex, "BankOfCanada"),
                        (LabelKey::Outcome, Outcome::EmptyMap.into()),
                    ],
                );
                assert_eq!(m.get(&key).copied(), Some(1));
            });
        }

        #[test]
        fn http_and_candid_errors_have_distinct_outcomes() {
            reset();
            let errors = vec![
                (
                    "BankOfItaly".to_string(),
                    CallForexError::Http {
                        forex: "BankOfItaly".to_string(),
                        error: "boom".to_string(),
                    },
                ),
                (
                    "CentralBankOfTurkey".to_string(),
                    CallForexError::Candid {
                        forex: "CentralBankOfTurkey".to_string(),
                        error: "nope".to_string(),
                    },
                ),
            ];
            record_per_forex_metrics(0, &[], &errors);

            with_labeled_counters(|m| {
                let http_key = make_metric_key(
                    MetricName::ForexFetchTotal,
                    &[
                        (LabelKey::Forex, "BankOfItaly"),
                        (LabelKey::Outcome, Outcome::HttpError.into()),
                    ],
                );
                let candid_key = make_metric_key(
                    MetricName::ForexFetchTotal,
                    &[
                        (LabelKey::Forex, "CentralBankOfTurkey"),
                        (LabelKey::Outcome, Outcome::CandidError.into()),
                    ],
                );
                assert_eq!(m.get(&http_key).copied(), Some(1));
                assert_eq!(m.get(&candid_key).copied(), Some(1));
            });
        }

        #[test]
        fn update_forex_store_success_sets_heartbeat_and_per_forex_metrics() {
            reset();
            let timestamp = 1_700_000_000;
            let map = btreemap! {
                "EUR".to_string() => 10_000,
                crate::forex::COMPUTED_XDR_SYMBOL.to_string() => 10_000,
            };
            let mock = MockForexSourcesImpl::new(
                vec![map.clone(), map.clone(), map.clone(), map],
                vec![],
            );
            update_forex_store(timestamp, &mock)
                .now_or_never()
                .expect("should execute");

            // Heartbeat gauge is set unconditionally on the Success return path.
            with_labeled_gauges(|m| {
                let heartbeat =
                    make_metric_key(MetricName::PeriodicForexRunLastSeconds, &[]);
                assert_eq!(m.get(&heartbeat).copied(), Some(timestamp as f64));
            });

            // The mock returns four success entries all named "src_name", so the
            // labeled counter should reflect that update_forex_store routed each
            // through record_per_forex_metrics.
            with_labeled_counters(|m| {
                let success_key = make_metric_key(
                    MetricName::ForexFetchTotal,
                    &[
                        (LabelKey::Forex, "src_name"),
                        (LabelKey::Outcome, Outcome::Success.into()),
                    ],
                );
                assert_eq!(m.get(&success_key).copied(), Some(4));
            });
        }

        #[test]
        fn failure_does_not_advance_last_success_gauge() {
            reset();
            let errors = vec![(
                "ReserveBankOfAustralia".to_string(),
                CallForexError::Http {
                    forex: "ReserveBankOfAustralia".to_string(),
                    error: "boom".to_string(),
                },
            )];
            record_per_forex_metrics(42, &[], &errors);

            with_labeled_gauges(|m| {
                let key = make_metric_key(
                    MetricName::ForexLastSuccessSeconds,
                    &[(LabelKey::Forex, "ReserveBankOfAustralia")],
                );
                assert!(
                    m.get(&key).is_none(),
                    "last_success gauge should not be set by a failure"
                );
            });
        }
    }
}
