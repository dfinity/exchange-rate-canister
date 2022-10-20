use std::cell::Cell;

use crate::{
    call_forex,
    forex::{ForexContextArgs, FOREX_SOURCES},
};

thread_local! {
    static FOREX_STORE_UPDATING: Cell<bool> = Cell::new(false);
}

pub async fn update_forex_store(timestamp: u64) {
    let is_updating = FOREX_STORE_UPDATING.with(|cell| cell.get());
    if is_updating {
        return;
    }

    FOREX_STORE_UPDATING.with(|cell| cell.set(true));

    ic_cdk::println!("periodic: {}", timestamp);

    call_forex_sources(timestamp).await;

    FOREX_STORE_UPDATING.with(|cell| cell.set(false));
}

async fn call_forex_sources(timestamp: u64) {
    let args = ForexContextArgs { timestamp };
    let forex = FOREX_SOURCES.get(0).unwrap();
    let response = call_forex(forex, &args).await;
    ic_cdk::println!("{:#?}", response);
}
