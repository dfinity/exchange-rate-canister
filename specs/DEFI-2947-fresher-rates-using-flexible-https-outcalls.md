---
id: DEFI-2947
title: Fresher rates using flexible HTTPS outcalls
tags: [xrc, https-outcalls, live-rates]
---

# Fresher rates using flexible HTTPS outcalls

## Motivation

The Exchange Rate Canister (XRC) today returns crypto rates for an **already-closed
minute** — in practice ~1–2.5 minutes stale. This is forced by *replicated* HTTPS
outcalls: every replica must agree byte-for-byte on each outcall response (after the
transform) for consensus to form. The currently-forming minute keeps changing as trades
arrive, so replicas issuing the outcall a few seconds apart see different bytes and fail
to agree. XRC therefore deliberately requests a settled, closed minute (see the rationale
comment in `exchanges.rs`, and the `now − 30s`-then-floor logic in `utils::get_normalized_timestamp`).

Some consumers cannot tolerate that staleness (it is too risky for their use). The
replica's new **flexible HTTPS outcalls** remove the byte-agreement requirement: a
committee of nodes each perform the outcall independently and the canister receives a
**vector** of (possibly differing) responses that it aggregates itself. This lets XRC
query the **forming minute** via real-time ticker endpoints and return a much fresher
"live" rate — the per-node divergence simply becomes an intra-exchange sample spread
rather than a consensus failure.

**Operational context.** `settled` (today's behavior) stays the default and remains the
mode used by consensus-critical consumers (the Cycles Minting Canister / NNS). `live` is
an **opt-in, best-effort** mode for latency-sensitive callers, initially limited to a
small allowlist of high-liquidity assets.

## Requirements

*Canonical behavioral contract. Per-PR acceptance criteria reference these `R#`.*

- **R1** — `get_exchange_rate` accepts a new optional request field `freshness : opt variant { Settled; Live }`. When absent, behavior is identical to today (⇒ `Settled`). Adding this `opt` field is Candid backward-compatible in both directions.
- **R2** — A `Settled` request produces byte-for-byte the same outcalls, aggregation, caching, timestamps, and outputs as before this feature (candle endpoints, closed minute, replicated outcalls). No settled output changes.
- **R3** — A `Live` request for an allowlisted asset queries the **forming minute** using real-time ticker endpoints via **flexible** outcalls with a **full-subnet committee**, and returns the **median across committee nodes per exchange, then the median across exchanges** (one vote per exchange), reusing the existing `QueriedExchangeRate` cross-exchange path.
- **R4** — `Live` is served only for assets in the live allowlist (`PRIVILEGED_CRYPTO_ASSETS` = `[BTC, ETH, ICP, USDC, USDT]`, plus `USDS` for the stablecoin bridge). A `Live` request naming a non-allowlisted asset is served as `Settled`.
- **R5** — XRC does **not** special-case caller identity for `freshness`. Consensus-critical consumers (CMC/NNS) obtain `Settled` by not opting in (R1) and by their own judgment; it is the caller's choice. (Server-side pinning of privileged callers was considered and rejected — see Discussed Alternatives.)
- **R6** — `Live` outcalls draw on a **separate** in-flight outcall budget (`LIVE_REQUEST_COUNTER_LIMIT`), independent of the existing settled `REQUEST_COUNTER_LIMIT` (56, unchanged). Settled throughput is never reduced by live load, and vice versa.
- **R7** — At most one in-flight `Live` fetch per **asset leg** (`<symbol>/USDT`) at a time (per-leg coalescing); up to `K = 5` concurrent distinct live fetches. A request that cannot be admitted within the live budget/coalescing rules returns `RateLimited` or `Pending` (never blocks or duplicates an in-flight leg).
- **R8** — `Live` rates are cached per asset with their observation time. A subsequent `Live` request for the same asset within `T = 10s` of the cached observation reuses the cached value and reports the **stored** observation time.
- **R9** — If the intra-exchange (committee) spread or the inter-exchange spread exceeds the configured thresholds, a `Live` request returns `InconsistentRatesReceived`. It does **not** silently fall back to a closed minute.
- **R10** — A `Live` response's `timestamp` is the **actual observation time**: `ic0.time()` captured after the outcalls resolve, **second-granular (not minute-floored)**. On a cache hit (R8), the stored observation time is returned.
- **R11** — The transform is retained in `Live` mode (each response reduced to the extracted rate) solely to minimize gossiped bytes; it is no longer relied on for determinism.
- **R12** — `Live` behavior is gated behind a **compile-time cargo feature**. When the feature is disabled, the `freshness` field is still accepted but `Live` is treated as `Settled`. Toggling live on/off is a rebuild + upgrade (this is the kill switch).
- **R13** — A **shadow-compute** path: when enabled, for a sampled fraction of `Settled` traffic (and all traffic on `beta`) the canister also computes the live rate **without returning it**, recording comparison metrics (R14). Shadow issues real outcalls and is therefore gated to the allowlist + a sample rate.
- **R14** — New Prometheus metrics are exposed on the existing `/metrics` query (see Implementation), and the privileged/non-privileged request logs and dashboard carry the `freshness` of each request.
- **R15** — On XRC's feeless system subnet, flexible outcalls cost XRC nothing (PAYG honors the `Free` cost schedule); the caller-facing `get_exchange_rate` cycles fee is unchanged.
- **R16** — Forex/fiat retrieval and all governance/CMC-facing behavior remain on classic replicated outcalls, unchanged.

## Non-goals

- **No change to `settled`** outputs or code paths in this effort (a later consolidation of `settled` onto flexible outcalls, once `live` is proven, is explicitly out of scope — tracked separately).
- **Forex/fiat stays replicated** (daily cadence, no freshness need).
- **No new response provenance metadata** beyond the `timestamp` semantics of R10; a `live` caller already knows it opted in. The existing `standard_deviation` / `num_received_rates` convey quality.
- **`Live` is not suitable for consensus-critical use** (documented in the Candid comment). Consensus-critical consumers are expected to stay on `Settled`, but this is not enforced by caller identity (R5).
- **No runtime kill switch** — the kill switch is the compile-time feature (R12).
- **In-repo integration testing of divergent per-node responses is out of scope here.** The existing harness (single dfx replica + nginx; the in-flight PocketIC branch pins `additional_responses = vec![]`) cannot mock a committee returning differing responses. This is tracked as a **separate blocking dependency ticket** (PocketIC/replica flexible-outcall mocking). Until it lands, the divergent-path integration tests are blocked; confidence comes from unit tests + shadow-compute + beta soak.
- **Accepted residual limitation:** a `Live` request for a non-allowlisted asset, or when the feature is off, silently returns a `Settled` rate (R4/R12) — considered acceptable for a beta.

## Design Decisions

- **Flexible committee, not single-replica.** `Live` uses the new `flexible_http_request` (committee = full subnet), not the shipped single-replica `is_replicated: false` mode — a single untrusted node returning arbitrary prices is unacceptable for a price oracle. Full subnet keeps guarantees as close to today as possible; a flexible request consumes only **one** subnet in-flight slot regardless of committee size.
- **Median-per-exchange, then median-across-exchanges** (one vote per exchange), rather than flattening all node observations into one vector (which over-weights exchanges with more responding nodes). The committee spread per exchange becomes a quality signal.
- **Ticker endpoints for `live` only.** `Live` uses each exchange's real-time last-trade ticker endpoint; `settled` keeps its candle/OHLC endpoints unchanged. Uniform ticker use avoids per-exchange candle quirks (OKX/Bitget `history-candles` cannot serve the forming minute anyway).
- **Opt-in `freshness`, default `settled`, caller decides.** Backward compatible; CMC/NNS are unaffected because they don't opt in — not because of a server-side pin (R5).
- **Separate live budget + per-leg coalescing + `T = 10s` cache.** Because a cross-subnet call already takes ~10–15s, a ≤10s-old rate is near the practical freshness floor; the cache collapses bursts to one fetch per asset per 10s and is the main protection for the outcall budget.
- **Second-granular observation time** (R10) — honest freshness reporting; differs from settled's minute granularity.
- **Compile-time feature gate** as the kill switch (R12), consistent with existing `ipv4-support` / `application-subnet` feature style; toggling requires rebuild + NNS upgrade.
- **Error, not fallback, on excessive spread** (R9).
- **Reuse `PRIVILEGED_CRYPTO_ASSETS`** as the allowlist (+`USDS`), rather than inventing a new set.

## Implementation

### Constraints

- The replica `flexible_http_request` API is **not yet in the public `ic.did`** and its field layout may still shift. Build against the `eichhorl/*` branches first; switch the dependency to `master` once merged.
- A flexible request = **1** in-flight outcall slot regardless of committee size (subnet cap 3000); it uses **pay-as-you-go** pricing, which is a no-op on a `Free` cost-schedule subnet (XRC's).
- The **production** XRC build enables `ipv4-support` (the Dockerfile leaves `IP_SUPPORT` unset ⇒ `build-wasm` defaults it to `ipv4`), so **9** exchanges are available, not 6.
- There is **no ic-cdk helper** for `flexible_http_request`; callers use `call_raw` + Candid encode/decode and compute cycles themselves.
- The test harness cannot yet mock divergent per-node responses (see Non-goals).
- In-canister metric counters reset on upgrade; long-run trends rely on externally-scraped `/metrics`. `*_last_success_seconds` gauges are seeded on init (`init_at`) to avoid false staleness alerts after upgrades.

### Types (`src/ic-xrc-types`) & Candid (`src/xrc/xrc.did`)

- Add `Freshness = variant { Settled; Live }` and `freshness : opt Freshness` to `GetExchangeRateRequest` (Rust + `.did`), with a disclaimer comment on the field: opt-in, best-effort, un-settled, non-deterministic across replicas, may change/be removed, not for consensus-critical use.
- Preserve `freshness` through `utils::sanitize_request` (which rebuilds the struct field-by-field and would otherwise drop it).

### Flexible outcall binding (new module, e.g. `src/xrc/src/flexible_http.rs`)

- Build `FlexibleCanisterHttpRequestArgs { url, headers, method: GET, body: None, transform, replication: Some(ReplicationCounts { total_requests = N, min_responses = 2N/3+1, max_responses = N }) }`; `call_raw(management_canister, "flexible_http_request", …)`; decode `FlexibleHttpRequestResult`.
- Map `Ok(Vec<CanisterHttpResponsePayload>)` → per-node observations; map `Err(FlexibleHttpRequestErr)` variants (`too_many_rejects`, `responses_too_large`, `timeout`, `out_of_cycles`, `invalid_parameters`) and per-node `node_details` to metrics + errors.
- Cycle computation (no helper); on a `Free` subnet attaching 0 works, mirroring `Exchange::cycles()`.

### Exchange ticker endpoints (`src/xrc/src/exchanges.rs`)

- Per exchange, add a real-time ticker URL + `extract_rate`-style parser used **only** in `live` mode; leave the candle endpoint/parser for `settled` untouched. Add `test-data/exchanges/<exchange>-ticker.json` fixtures.
- Pin new ticker URLs in the `get_url`/query-string tests.

### Aggregation & spread (`src/xrc/src/lib.rs`)

- New: median across committee-node observations → one rate per exchange, feeding the existing `QueriedExchangeRate` cross-exchange median (`Mul`/`Div`, ±20% band, final median + `standard_deviation`).
- Spread checks (R9): intra-exchange committee spread and inter-exchange spread thresholds → `InconsistentRatesReceived`.

### Live cache & coalescing (`src/xrc/src/cache.rs`, `src/xrc/src/inflight.rs`)

- New live cache: `asset → (QueriedExchangeRate, fetched_at_secs)`, distinct from the immutable `(asset, minute)` settled cache; serve on hit when `now − fetched_at ≤ T` (10s), else refetch and re-stamp.
- Per-leg live inflight set; admit up to `K = 5` concurrent; beyond ⇒ `Pending`.

### Rate limiting (`src/xrc/src/rate_limiting.rs`)

- New `LIVE_REQUEST_COUNTER_LIMIT` (~90, counted in outcalls: 9 exchanges × up to 2 legs ⇒ ~5 concurrent pair fetches), with its own RAII counter guard, separate from the settled 56.

### API routing (`src/xrc/src/api.rs`)

- Route on `freshness`: allowlist gate (R4) → feature gate (R12) → live path vs settled path (no caller-identity special-casing, R5); assign the R10 timestamp; charge unchanged caller fee (R15).

### Shadow-compute (`src/xrc/src/api.rs` + metrics)

- Sampled dual computation of the live rate alongside settled, results not returned, metrics recorded (R13/R14). Feature/config-gated + sample rate.

### Metrics & request log (`src/xrc/src/api/metrics.rs`, `lib.rs`, `request_log.rs`, `api/dashboard.rs`)

New series (mirroring the existing labeled `xrc_*` families):

| Metric | Type / labels | Purpose |
|---|---|---|
| `xrc_live_requests_total{result}` | counter | ok/rate_limited/inconsistent/pending/no_data/error |
| `xrc_live_vs_settled_rel_diff{asset}` | histogram | primary quality signal (populated by shadow pre-exposure) |
| `xrc_live_intra_exchange_spread{exchange}` | histogram | committee-node divergence |
| `xrc_live_committee_responses{exchange,outcome}` | counter | OK vs reject/timeout among min..max |
| `xrc_flexible_outcall_total{exchange,global_error}` | counter | flexible-specific failures |
| `xrc_live_exchange_http_status{exchange,status}` | counter | ticker HTTP status — 429/403 rate-limit/ban detection |
| `xrc_live_cache_hits_total` / `_misses_total` | counter | cache effectiveness |
| `xrc_live_outcall_counter` | gauge | live-budget usage |
| `xrc_live_last_success_seconds{exchange}` | gauge | ticker-path staleness (seed via `init_at`) |

Add a `freshness` column to the request-log tables / dashboard.

Alerts (defined in the external k8s/monitoring repo, **beta severity** initially): live-vs-settled divergence; ticker 429/403 per exchange; flexible `too_many_rejects`/`timeout`; live-budget saturation; spurious inconsistency in calm markets; live staleness (mirror `IC_XRC_ExchangeSilent`); and an **isolation guard** alert if settled `RateLimited`/error/latency regress after live ships.

### Feature gate (`src/xrc/Cargo.toml`, `scripts/build-wasm`, `Dockerfile`)

- New cargo feature (e.g. `live-rates`) gating all `live` behavior (R12); wire an opt-in build arg in `build-wasm`/`Dockerfile` so the `beta` build can enable it while production stays off until ready.

### Testing

- **Unit (in this ticket):** ticker parsers (fixtures); committee-median-per-exchange + spread→error (extend in-memory `QueriedExchangeRate` tests to feed per-node vectors); cache TTL/`fetched_at`; per-leg coalescing; separate budget; freshness routing; allowlist gating; privileged pinning; feature-off ⇒ settled; timestamp assignment; flexible error mapping.
- **Integration:** extend the mocked harness to cover the ticker URL/parse changes (single-response) now; the **divergent-path** integration tests are **blocked** on the separate harness dependency ticket.
- **Shadow-compute** (R13) is the primary pre-exposure de-risking: real exchanges, real committee, no caller exposure.
- **Beta soak:** enable the feature on the `beta` canister; point `monitor-canister` at it (extend pairs to the allowlist, request `live`, log live-vs-settled). ~2–4 weeks spanning weekdays, ≥1 weekend, and ≥1 genuine volatility episode.
- **Capacity load:** a driver canister on a different subnet issuing concurrent `live` requests (with the T=10s cache in the SUT) ramped to ~2–5× peak, watching budget saturation and exchange 429/403.

### Edge cases

- All/most committee nodes reject or the exchange is down → `too_many_rejects` → error (R9-adjacent).
- Thin liquidity / very start of a minute → high spread → `InconsistentRatesReceived` (R9).
- Ticker endpoint rate-limits the committee (429/403) → counted as outcall failure; if OK responses fall below `min_responses`, error; metric `xrc_live_exchange_http_status` fires.
- Cache hit within T returns a stale-but-honest older `timestamp` (R8/R10).
- Non-allowlisted asset requested `live` → served `settled` (R4); feature disabled → `live` == `settled` (R12). A privileged caller that opts into `live` for an allowlisted asset receives `live` — it is expected not to opt in (R5).
- Concurrent burst for the same asset leg → coalesced to one fetch or `Pending` (R7).
- `USDS` (not in `PRIVILEGED_CRYPTO_ASSETS`) must be added to the live allowlist for the stablecoin bridge to run live (R4).

### Delivery / PR sequence

Each PR is independently mergeable/compilable/testable. `R#` = requirements covered.

1. **PR1 — Types & plumbing (no behavior).** `freshness` field in `ic-xrc-types` + `.did` (with disclaimer); thread through `sanitize_request`, request log, dashboard column; add the cargo feature scaffold. Feature off ⇒ `Live` == `Settled`. Covers R1, R12 (off path), part of R14.
2. **PR2 — Flexible outcall binding.** `flexible_http.rs` + unit tests for arg construction, cycle calc, result/error decoding. Covers R11 (transform retained), binding for R3/R15.
3. **PR3 — Ticker endpoints.** Per-exchange ticker URL + parser (feature-gated) + fixtures + parser/URL tests. Settled untouched (R2).
4. **PR4 — Aggregation & spread.** Committee-median-per-exchange feeding the cross-exchange median; spread→`InconsistentRatesReceived`. Covers R3, R9.
5. **PR5 — Cache, coalescing, budget.** Live cache (T=10s, `fetched_at`), per-leg inflight (K=5), separate `LIVE_REQUEST_COUNTER_LIMIT`. Covers R6, R7, R8.
6. **PR6 — API routing.** `freshness` routing, allowlist gate, R10 timestamp, unchanged fee, no caller-identity special-casing. Covers R4, R5, R10, R15, R16 (confirm forex untouched).
7. **PR7 — Shadow-compute, metrics, alerts.** Shadow path + all new metrics + log/dashboard freshness; alert definitions handed to the monitoring repo. Covers R13, R14.
8. **Rollout (not a code PR):** beta soak → enable the feature in the production build via NNS upgrade → document as supported (still opt-in) after a clean monitored period.

**Blocking dependency (separate ticket):** flexible-outcall test harness (PocketIC/replica divergent-response mocking) — unblocks divergent-path integration tests.

## Discussed Alternatives

- **Single-replica `is_replicated: false`** — rejected: one untrusted node defining an oracle price.
- **Flatten all node observations into one vector** — rejected: over-weights exchanges with more responding nodes; chose median-per-exchange.
- **Keep candle endpoints for `live`** (drop the 60s trim) — viable only for Coinbase/KuCoin; OKX/Bitget `history-candles` can't serve the forming minute. Chose uniform ticker endpoints.
- **Runtime kill-switch flag** — considered; user chose a compile-time feature instead (rebuild + upgrade to toggle).
- **Fallback to the last closed minute on high spread** — rejected (R9): return `InconsistentRatesReceived` instead, so callers are never silently handed a stale value labeled fresh.
- **Response provenance metadata** (rate_source / forming_minute flags) — deferred: the opt-in `freshness` field already tells the caller what it asked for.
- **Server-side pinning of privileged callers to `settled`** — considered as a safety interlock (structurally prevent a forming-minute rate from ever reaching cycles-minting). Rejected: it contradicts the opt-in model, would silently override a caller that explicitly requested `live`, and is the only caller-identity special-case. The default (R1) plus the allowlist and beta feature gate already keep CMC/NNS on `settled`; choosing `settled` is the caller's responsibility.
- **Silent production rollout without safeguards** — rejected: the disclaimer comment is not real protection; the allowlist, path isolation (separate budget/cache), compile-time gate, and live metrics/alerts are.
- **Bumping the settled `REQUEST_COUNTER_LIMIT` (56)** to share one budget — rejected in favor of a separate live counter so live and settled cannot starve each other.
