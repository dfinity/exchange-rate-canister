use ic_xrc_types::{Asset, AssetClass};

mod basic_exchange_rates;
mod caching;
mod determinism;
mod get_icp_xdr_rate;
mod misbehavior;
mod real_world;

// Temporary solution until a refactor is done to create a test helper crate
// that will contain these functions to be used in `xrc` and `xrc-tests`.
fn btc_asset() -> Asset {
    Asset {
        symbol: "BTC".to_string(),
        class: AssetClass::Cryptocurrency,
    }
}

fn eur_asset() -> Asset {
    Asset {
        symbol: "EUR".to_string(),
        class: AssetClass::FiatCurrency,
    }
}

fn icp_asset() -> Asset {
    Asset {
        symbol: "ICP".to_string(),
        class: AssetClass::Cryptocurrency,
    }
}
