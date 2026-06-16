//! Instruction benchmarks for the per-exchange listing parsers, measured with
//! `canbench` (see `canbench.yml`).
//!
//! The listing parse runs inside the HTTP-outcall transform — a single,
//! non-DTS execution whose instruction budget is the binding constraint when
//! refreshing the (up to ~1.2 MiB) listing endpoints — so this module
//! benchmarks `extract_listed_usdt_bases` on real, full-size captures
//! (`benches/data/`, the worst case being OKX at ~1.2 MiB). They call the
//! production parser
//! directly, so a regression in the canister's listing parse moves these
//! numbers and trips the CI benchmark check.

use canbench_rs::{bench, BenchResult};

/// Measure parsing a captured listing for the named exchange, mirroring exactly
/// what the refresh transform would run. Only the parse is timed; locating the
/// exchange (and the compile-time `include_bytes!`) is outside `bench_fn`.
fn measure_parse(exchange_name: &str, body: &'static [u8]) -> BenchResult {
    let exchange = xrc::EXCHANGES
        .iter()
        .find(|exchange| exchange.name() == exchange_name)
        .expect("benchmark names an unknown exchange");
    canbench_rs::bench_fn(|| {
        let listed = exchange
            .extract_listed_usdt_bases(std::hint::black_box(body))
            .expect("captured listing should parse");
        std::hint::black_box(listed);
    })
}

macro_rules! listing_bench {
    ($name:ident, $exchange:literal, $file:literal) => {
        #[bench(raw)]
        fn $name() -> BenchResult {
            measure_parse($exchange, include_bytes!($file))
        }
    };
}

// Ordered worst-case (largest raw payload) first.
listing_bench!(listing_parse_okx, "Okx", "../benches/data/okx.json");
listing_bench!(listing_parse_gateio, "GateIo", "../benches/data/gateio.json");
listing_bench!(listing_parse_kucoin, "KuCoin", "../benches/data/kucoin.json");
listing_bench!(listing_parse_poloniex, "Poloniex", "../benches/data/poloniex.json");
listing_bench!(listing_parse_cryptocom, "CryptoCom", "../benches/data/cryptocom.json");
listing_bench!(listing_parse_bitget, "Bitget", "../benches/data/bitget.json");
listing_bench!(listing_parse_coinbase, "Coinbase", "../benches/data/coinbase.json");
listing_bench!(listing_parse_digifinex, "Digifinex", "../benches/data/digifinex.json");
listing_bench!(listing_parse_mexc, "Mexc", "../benches/data/mexc.json");
