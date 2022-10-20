use std::cell::Cell;

use crate::{
    call_forex,
    forex::{collect_rates, ForexContextArgs, ForexRateMap, FOREX_SOURCES},
    with_forex_rate_store_mut,
};

thread_local! {
    static NEXT_RUN_SCHEDULE_AT: Cell<u64> = Cell::new(0);
    static FOREX_STORE_UPDATING: Cell<bool> = Cell::new(false);
}

const ONE_MINUTE: u64 = 60;
const ONE_HOUR: u64 = 60 * ONE_MINUTE;
const SIX_HOURS: u64 = 6 * ONE_HOUR;
const ONE_DAY: u64 = 24 * ONE_HOUR;

pub async fn update_forex_store(timestamp: u64) {
    let next_run_at = NEXT_RUN_SCHEDULE_AT.with(|cell| cell.get());
    if next_run_at == 0 {}

    if next_run_at > 0 && next_run_at > timestamp {
        return;
    }

    let is_updating = FOREX_STORE_UPDATING.with(|cell| cell.get());
    if is_updating {
        return;
    }

    FOREX_STORE_UPDATING.with(|cell| cell.set(true));

    ic_cdk::println!("periodic: {}", timestamp);

    let forex_rate_maps = call_forex_sources(timestamp).await;
    let forex_multi_rate_map = collect_rates(timestamp, forex_rate_maps);
    with_forex_rate_store_mut(|store| store.put(timestamp, forex_multi_rate_map));

    FOREX_STORE_UPDATING.with(|cell| cell.set(false));
    NEXT_RUN_SCHEDULE_AT.with(|cell| cell.set(timestamp + 20))
}

async fn call_forex_sources(timestamp: u64) -> Vec<ForexRateMap> {
    let args = ForexContextArgs { timestamp };
    let forex = FOREX_SOURCES.get(0).unwrap();
    let response = call_forex(forex, &args).await.unwrap();
    ic_cdk::println!("{:#?}", response);
    vec![response]
}
