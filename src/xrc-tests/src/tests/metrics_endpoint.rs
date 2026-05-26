use ic_xrc_types::{Asset, AssetClass, GetExchangeRateRequest, GetExchangeRateResult};
use maplit::hashmap;
use xrc::types::{HttpRequest, HttpResponse};

use crate::{
    container::{run_scenario, Container},
    mock_responses,
};

/// Setup:
/// * Deploy mock FOREX data providers and exchanges, with one exchange
///   configured to return no body (driving a non-success outcome on the
///   exchange counter).
/// * Start replicas and deploy the XRC, configured to use the mock data
///   sources.
///
/// Runbook:
/// * Drive a `get_exchange_rate` request so the recording sites fire across
///   the crypto, stablecoin, and forex code paths.
/// * Scrape the `/metrics` endpoint via the canister's `http_request` query.
///
/// Success criteria:
/// * Every labeled metric family is present on `/metrics`:
///   `xrc_exchange_fetch_total` (with both success and a non-success outcome
///   represented), `xrc_exchange_last_success_seconds`,
///   `xrc_forex_fetch_total`, `xrc_forex_last_success_seconds`,
///   `xrc_periodic_forex_run_last_seconds`, and
///   `xrc_stablecoin_symbol_rates_received`.
/// * Each labeled metric name emits exactly one `# HELP` line — regression
///   guard on the labeled-encoder header dedup logic.
// TODO(DEFI-2828): Replace `#[ignore]` with a clearer gate (reason string /
// feature flag) across the xrc-tests crate so the intent — "requires the
// Docker mock harness via ./scripts/e2e-tests" — is explicit rather than
// implicit.
#[ignore]
#[test]
fn metrics_endpoint_exposes_all_labeled_series() {
    let now_seconds = time::OffsetDateTime::now_utc().unix_timestamp() as u64;
    let timestamp_seconds = now_seconds / 60 * 60;

    let responses = mock_responses::exchanges::build_responses(
        "ICP".to_string(),
        timestamp_seconds,
        |exchange| match exchange {
            // KuCoin returns no body — drives a non-success outcome on the
            // crypto fetch path for this exchange.
            xrc::Exchange::KuCoin(_) => None,
            xrc::Exchange::Coinbase(_) => Some("3.92"),
            xrc::Exchange::Okx(_) => Some("3.90"),
            xrc::Exchange::GateIo(_) => Some("3.90"),
            xrc::Exchange::Mexc(_) => Some("3.911"),
            xrc::Exchange::Poloniex(_) => Some("4.005"),
            xrc::Exchange::CryptoCom(_) => Some("3.91"),
            xrc::Exchange::Bitget(_) => Some("3.93"),
            xrc::Exchange::Digifinex(_) => Some("4.00"),
        },
    )
    .chain(mock_responses::stablecoin::build_responses(
        timestamp_seconds,
    ))
    .chain(mock_responses::forex::build_responses(
        now_seconds,
        |_| Some(hashmap! { "EUR" => "1.05" }),
    ))
    .collect::<Vec<_>>();

    let container = Container::builder()
        .name("metrics_endpoint")
        .exchange_responses(responses)
        .build();

    run_scenario(container, |container| {
        // Drive a request so the per-exchange and per-stablecoin recording
        // sites fire. The inner `GetExchangeRateResult` is intentionally not
        // asserted on — value correctness lives in basic_exchange_rates.rs /
        // misbehavior.rs. The outer call itself is `expect`-ed so an
        // infrastructure failure (dfx error, candid decode) fails the test
        // with a clear root-cause instead of silently propagating into the
        // metrics assertions below.
        let request = GetExchangeRateRequest {
            timestamp: Some(timestamp_seconds),
            base_asset: Asset {
                symbol: "ICP".to_string(),
                class: AssetClass::Cryptocurrency,
            },
            quote_asset: Asset {
                symbol: "EUR".to_string(),
                class: AssetClass::FiatCurrency,
            },
        };
        let _ = container
            .call_canister::<_, GetExchangeRateResult>("get_exchange_rate", &request)
            .expect("get_exchange_rate canister call must succeed");

        // Scrape /metrics via the canister's http_request endpoint.
        let response = container
            .call_canister::<HttpRequest, HttpResponse>(
                "http_request",
                &HttpRequest {
                    method: "GET".to_string(),
                    url: "/metrics".to_string(),
                    headers: vec![],
                    body: Default::default(),
                },
            )
            .expect("Failed to scrape /metrics");

        assert_eq!(response.status_code, 200, "/metrics returned non-200");
        let body =
            String::from_utf8(response.body.into_vec()).expect("/metrics body must be UTF-8");

        // Per-exchange counter — success for a healthy exchange, non-success
        // for the failing one. Assertions include the full `{exchange, kind,
        // outcome}` label set so a regression that silently drops or renames
        // one of those label keys fails the test instead of slipping through
        // on a name-only prefix match.
        assert!(
            body.contains(
                r#"xrc_exchange_fetch_total{exchange="Coinbase",kind="crypto",outcome="success"}"#
            ),
            "missing Coinbase crypto success series\n{body}"
        );
        let kucoin_failure_line = body.lines().find(|line| {
            line.starts_with(r#"xrc_exchange_fetch_total{exchange="KuCoin","#)
                && line.contains(r#"kind="crypto""#)
                && !line.contains(r#"outcome="success""#)
        });
        assert!(
            kucoin_failure_line.is_some(),
            "missing non-success outcome for KuCoin crypto\n{body}"
        );

        // Per-exchange last-success gauge — present at minimum because init
        // seeded it; ideally bumped by the successful Coinbase fetch.
        assert!(
            body.contains(r#"xrc_exchange_last_success_seconds{exchange="Coinbase""#),
            "missing xrc_exchange_last_success_seconds for Coinbase\n{body}"
        );

        // Per-forex counter — at least one success line. The empty_map and
        // other non-success outcomes are exercised in unit tests
        // (`periodic::test::empty_map_error_is_recorded_as_empty_map_outcome`
        // and friends); here we only verify that the labeled-encoder pipe
        // reaches `/metrics`.
        assert!(
            body.lines().any(|line| line
                .starts_with("xrc_forex_fetch_total{")
                && line.contains(r#"outcome="success""#)),
            "missing success outcome on the forex counter\n{body}"
        );

        // Per-forex last-success gauge — seeded by init, present for every
        // configured source.
        assert!(
            body.contains("xrc_forex_last_success_seconds{forex="),
            "missing xrc_forex_last_success_seconds series\n{body}"
        );

        // Stablecoin per-symbol counters — both bases.
        assert!(
            body.contains(r#"xrc_stablecoin_symbol_rates_received{symbol="USDS"}"#),
            "missing stablecoin counter for USDS\n{body}"
        );
        assert!(
            body.contains(r#"xrc_stablecoin_symbol_rates_received{symbol="USDC"}"#),
            "missing stablecoin counter for USDC\n{body}"
        );

        // Periodic-task heartbeat — set on every Success return of
        // update_forex_store.
        assert!(
            body.contains("xrc_periodic_forex_run_last_seconds"),
            "missing heartbeat gauge\n{body}"
        );

        // Each labeled metric name should emit exactly one `# HELP` line,
        // regardless of how many series share that name.
        for name in [
            "xrc_exchange_fetch_total",
            "xrc_exchange_last_success_seconds",
            "xrc_forex_fetch_total",
            "xrc_forex_last_success_seconds",
            "xrc_periodic_forex_run_last_seconds",
            "xrc_stablecoin_symbol_rates_received",
        ] {
            let help_prefix = format!("# HELP {} ", name);
            let help_count = body.lines().filter(|l| l.starts_with(&help_prefix)).count();
            assert_eq!(
                help_count, 1,
                "expected exactly one `# HELP {name}` line, found {help_count}\n{body}"
            );
        }

        Ok(())
    })
    .expect("Scenario failed");
}
