use xrc::{EXCHANGES, FOREX_SOURCES};

mod basic_exchange_rates;
mod caching;
mod determinism;
mod get_icp_xdr_rate;
mod misbehavior;

/// The total number of exchanges.
const NUM_EXCHANGES: usize = EXCHANGES.len();

/// The total number of forex data sources.
const NUM_FOREX_SOURCES: usize = FOREX_SOURCES.len();
