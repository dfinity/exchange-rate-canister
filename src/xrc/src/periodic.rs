use std::cell::Cell;

use async_trait::async_trait;
use futures::future::join_all;

use crate::{
    call_forex,
    forex::{collect_rates, ForexContextArgs, ForexRateMap, FOREX_SOURCES},
    with_forex_rate_store_mut, CallForexError,
};

thread_local! {
    static NEXT_RUN_SCHEDULE_AT: Cell<u64> = Cell::new(0);
    static FOREX_STORE_UPDATING: Cell<bool> = Cell::new(false);
}

const ONE_MINUTE: u64 = 60;
const ONE_HOUR: u64 = 60 * ONE_MINUTE;
const SIX_HOURS: u64 = 6 * ONE_HOUR;
const ONE_DAY: u64 = 24 * ONE_HOUR;

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

pub async fn run_tasks(timestamp: u64) {
    let forex_sources = ForexSourcesImpl;
    update_forex_store(timestamp, &forex_sources).await;
}

async fn update_forex_store(timestamp: u64, forex_sources: &impl ForexSources) {
    let next_run_at = NEXT_RUN_SCHEDULE_AT.with(|cell| cell.get());
    // TODO: may need a method to seed the next run value on initialization
    if next_run_at > 0 && next_run_at > timestamp {
        return;
    }

    let is_updating = FOREX_STORE_UPDATING.with(|cell| cell.get());
    if is_updating {
        return;
    }

    FOREX_STORE_UPDATING.with(|cell| cell.set(true));

    ic_cdk::println!("periodic: {}", timestamp);

    // TODO: track errors using a counter for each forex
    let today_timestamp = today_timestamp(timestamp);
    // TODO: what should happen if the forex_rate_maps vector is empty?
    let (forex_rate_maps, _) = forex_sources.call(today_timestamp).await;
    let forex_multi_rate_map = collect_rates(today_timestamp, forex_rate_maps);
    with_forex_rate_store_mut(|store| store.put(today_timestamp, forex_multi_rate_map));

    FOREX_STORE_UPDATING.with(|cell| cell.set(false));
    NEXT_RUN_SCHEDULE_AT.with(|cell| cell.set(get_next_run_time(timestamp)))
}

fn today_timestamp(timestamp: u64) -> u64 {
    timestamp - (timestamp % ONE_DAY)
}

fn get_next_run_time(timestamp: u64) -> u64 {
    (timestamp - (timestamp % SIX_HOURS)) + SIX_HOURS
}

#[cfg(test)]
mod test {}
