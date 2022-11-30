mod api;
mod environment;
mod periodic;
mod state;
pub mod types;

use state::init_config;
use types::Config;

pub use api::get_entries;

pub fn init(config: Config) {
    init_config(config)
}

pub fn heartbeat() {
    periodic::beat();
}
