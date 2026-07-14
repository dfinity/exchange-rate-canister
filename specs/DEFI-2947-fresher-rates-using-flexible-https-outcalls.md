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
- **R4** — `Live` is served only for assets in the live allowlist (`PRIVILEGED_CRYPTO_ASSETS` = `[BTC, ETH, ICP, USDC, USDT]`, plus `USDS` for the stablecoin bridge). A `Live` request naming a non-allowlisted asset returns an **error** (`ExchangeRateError::Other` with a dedicated code), not a silent `Settled` downgrade.
- **R5** — XRC does **not** special-case caller identity for `freshness`. Consensus-critical consumers (CMC/NNS) obtain `Settled` by not opting in (R1) and by their own judgment; it is the caller's choice. (Server-side pinning of privileged callers was considered and rejected — see Discussed Alternatives.)
- **R6** — `Live` outcalls draw on a **separate** in-flight outcall budget (`LIVE_REQUEST_COUNTER_LIMIT`), independent of the existing settled `REQUEST_COUNTER_LIMIT` (56, unchanged). Settled throughput is never reduced by live load, and vice versa.
- **R7** — `Live` fetch concurrency is bounded and de-duplicated: at most **one** in-flight fetch per asset leg (`<symbol>/USDT`) — a concurrent request for a leg already being fetched returns `Pending` (or a still-fresh cached value), never a duplicate outcall — and at most `K = 5` concurrent distinct-leg fetches within the live budget, beyond which a request returns `RateLimited`.
- **R8** — `Live` rates are cached per asset with their observation time. A subsequent `Live` request for the same asset within `T = 10s` of the cached observation reuses the cached value and reports the **stored** observation time.
- **R9** — If the intra-exchange (committee) spread or the inter-exchange spread exceeds the configured thresholds, a `Live` request returns `InconsistentRatesReceived`. It does **not** silently fall back to a closed minute.
- **R10** — A `Live` response's `timestamp` is the **actual observation time**: `ic0.time()` captured after the outcalls resolve, **second-granular (not minute-floored)**. On a cache hit (R8), the stored observation time is returned.
- **R11** — The transform is retained in `Live` mode (each response reduced to the extracted rate) solely to minimize gossiped bytes; it is no longer relied on for determinism.
- **R12** — `Live` behavior is gated behind a **compile-time cargo feature**. When the feature is disabled, the `freshness` field is still accepted, but a `Live` request returns an **error** (`ExchangeRateError::Other`, same dedicated code as R4 with a distinct description) rather than a silent `Settled` downgrade. A request that omits `freshness` is unaffected (still `Settled`). Toggling live on/off is a rebuild + upgrade (this is the kill switch).
- **R13** — A **shadow-compute** path: when enabled, for a sampled fraction of `Settled` traffic (and all traffic on `beta`) the canister also computes the live rate **without returning it**, recording comparison metrics (R14). Shadow issues real outcalls and is therefore gated to the allowlist + a sample rate.
- **R14** — New Prometheus metrics are exposed on the existing `/metrics` query (see Implementation), and the privileged/non-privileged request logs and dashboard carry the `freshness` of each request.
- **R15** — Flexible outcalls cost XRC nothing on its `Free`-cost-schedule subnet, and the caller-facing `get_exchange_rate` cycles fee is unchanged. On a `Free` subnet the enablement branch routes flexible requests through the **legacy no-charge** pricing path (`get_own_cost_schedule() == Free ⇒ PricingVersion::Legacy`), and current master's PAYG likewise short-circuits `charge()` when `Free` — free either way. Note the initial availability is **free-subnet-only**: on a `Normal` subnet the enable branch routes to PAYG, which is not yet implemented and is rejected downstream (so the failure mode there is "unavailable", never "unexpectedly charged").
- **R16** — Forex/fiat retrieval and all governance/CMC-facing behavior remain on classic replicated outcalls, unchanged.

## Non-goals

- **No change to `settled`** outputs or code paths in this effort (a later consolidation of `settled` onto flexible outcalls, once `live` is proven, is explicitly out of scope — tracked separately).
- **Forex/fiat stays replicated** (daily cadence, no freshness need).
- **No new response provenance metadata** beyond the `timestamp` semantics of R10; a `live` caller already knows it opted in. The existing `standard_deviation` / `num_received_rates` convey quality.
- **`Live` is not suitable for consensus-critical use** (documented in the Candid comment). Consensus-critical consumers are expected to stay on `Settled`, but this is not enforced by caller identity (R5).
- **No runtime kill switch** — the kill switch is the compile-time feature (R12).
- **In-repo integration testing of divergent per-node responses is out of scope here.** The existing harness (single dfx replica + nginx; the in-flight PocketIC branch pins `additional_responses = vec![]`) cannot mock a committee returning differing responses. This is tracked as a **separate blocking dependency ticket** (PocketIC/replica flexible-outcall mocking). Until it lands, the divergent-path integration tests are blocked; confidence comes from unit tests + shadow-compute + beta soak.
- `Live` is intentionally **not** offered for every asset (allowlist only, R4) nor in a feature-off build (R12); those cases return an explicit error rather than a silent `Settled` result. Omitting `freshness` always yields `Settled` and is never an error.

## Design Decisions

- **Flexible committee, not single-replica.** `Live` uses the new `flexible_http_request` (committee = full subnet), not the shipped single-replica `is_replicated: false` mode — a single untrusted node returning arbitrary prices is unacceptable for a price oracle. Full subnet keeps guarantees as close to today as possible; a flexible request consumes only **one** subnet in-flight slot regardless of committee size.
- **Median-per-exchange, then median-across-exchanges** (one vote per exchange), rather than flattening all node observations into one vector (which over-weights exchanges with more responding nodes). The committee spread per exchange becomes a quality signal.
- **Ticker endpoints for `live` only.** `Live` uses each exchange's real-time last-trade ticker endpoint; `settled` keeps its candle/OHLC endpoints unchanged. Uniform ticker use avoids per-exchange candle quirks (OKX/Bitget `history-candles` cannot serve the forming minute anyway).
- **Opt-in `freshness`, default `settled`, caller decides.** Backward compatible; CMC/NNS are unaffected because they don't opt in — not because of a server-side pin (R5).
- **Separate live budget + per-leg coalescing + `T = 10s` cache.** Because a cross-subnet call already takes ~10–15s, a ≤10s-old rate is near the practical freshness floor; the cache collapses bursts to one fetch per asset per 10s and is the main protection for the outcall budget.
- **Second-granular observation time** (R10) — honest freshness reporting; differs from settled's minute granularity.
- **Compile-time feature gate** as the kill switch (R12), consistent with existing `ipv4-support` / `application-subnet` feature style; toggling requires rebuild + NNS upgrade.
- **Error, not fallback, on excessive spread** (R9).
- **Explicit errors, never silent downgrades.** A `Live` request that cannot be served as live (non-allowlisted asset R4, feature off R12) returns an error, not a `Settled` rate the caller didn't ask to receive. (Same reasoning that removed the R5 pin.)
- **Reuse `PRIVILEGED_CRYPTO_ASSETS`** as the allowlist (+`USDS`), rather than inventing a new set.

## Implementation

### Constraints

- The replica `flexible_http_request` API is **not yet in the public `ic.did`** and its field layout may still shift. Build against the `eichhorl/*` branches first; switch the dependency to `master` once merged.
- A flexible request = **1** in-flight outcall slot regardless of committee size (subnet cap 3000). Pricing follows the subnet's cost schedule (`get_own_cost_schedule()`): on a `Free`-cost-schedule subnet (XRC's, inferred from XRC attaching 0 cycles today and working) the enable-on-free-subnets branch routes to the **legacy no-charge** path — free; on a `Normal` subnet that branch routes to PAYG, which is unimplemented and rejected downstream. Net: the feature is **initially free-subnet-only**, and free there.
- The **production** XRC build enables `ipv4-support` (the Dockerfile leaves `IP_SUPPORT` unset ⇒ `build-wasm` defaults it to `ipv4`), so **9** exchanges are available, not 6.
- There is **no ic-cdk helper** for `flexible_http_request`; callers use `call_raw` + Candid encode/decode and compute cycles themselves.
- The test harness cannot yet mock divergent per-node responses (see Non-goals).
- In-canister metric counters reset on upgrade; long-run trends rely on externally-scraped `/metrics`. `*_last_success_seconds` gauges are seeded on init (`init_at`) to avoid false staleness alerts after upgrades.

### Types (`src/ic-xrc-types`) & Candid (`src/xrc/xrc.did`)

- Add `Freshness = variant { Settled; Live }` and `freshness : opt Freshness` to `GetExchangeRateRequest` (Rust + `.did`), with a disclaimer comment on the field: opt-in, best-effort, un-settled, non-deterministic across replicas, may change/be removed, not for consensus-critical use.
- Preserve `freshness` through `utils::sanitize_request` (which rebuilds the struct field-by-field and would otherwise drop it).
- Add a dedicated "live rate unavailable" error code in `errors.rs`, surfaced via `ExchangeRateError::Other` (the repo's convention for post-launch errors, per the `xrc.did` comment) — used for both the non-allowlisted (R4) and feature-off (R12) cases, distinguished by description (or two codes).

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
- **Single-flight coalescing (chosen mechanism):** reuse the existing inflight set (keyed by `symbol` / `(symbol, LIVE)`). The first `Live` request for a leg sets the inflight flag and issues the one flexible fetch, populates the cache, then clears the flag. The check-then-set has no `await` between the two, so it is atomic — IC messages do not interleave except at `await` points, so there is no TOCTOU race. Concurrent requests for the same leg return `Pending` and retry onto the warm T=10s cache. The IC provides no way to resume a second caller on the *first* caller's outcall completion (a call resumes only on its own outstanding syscall), so blocking-and-sharing is deliberately not attempted (see Discussed Alternatives). Admit up to `K = 5` concurrent distinct-leg fetches; beyond the live budget ⇒ `RateLimited`.
- **Trap-safety:** set the inflight flag via an RAII guard (like the existing `RateLimitingRequestCounterGuard`) so it clears on any early return. The flag is committed at the first `await` boundary, so a trap during response processing would otherwise wedge the leg; a failed/timed-out outcall returns normally and must run the cleanup.

### Rate limiting (`src/xrc/src/rate_limiting.rs`)

- New `LIVE_REQUEST_COUNTER_LIMIT` (~90, counted in outcalls: 9 exchanges × up to 2 legs ⇒ ~5 concurrent pair fetches), with its own RAII counter guard, separate from the settled 56.

### API routing (`src/xrc/src/api.rs`)

- Route on `freshness`: if `Live`, require the feature enabled (R12) and the asset allowlisted (R4) — otherwise return the dedicated `Other` error; else run the settled path. No caller-identity special-casing (R5). Assign the R10 timestamp; charge unchanged caller fee (R15).

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
- Non-allowlisted asset requested `live` → error (R4); feature disabled + `live` requested → error (R12); `freshness` omitted → `settled`, never an error. A privileged caller that opts into `live` for an allowlisted asset receives `live` — it is expected not to opt in (R5).
- Concurrent burst for the same asset leg → exactly one fetch; concurrent callers get `Pending` and retry onto the warm T=10s cache (R7).
- `USDS` (not in `PRIVILEGED_CRYPTO_ASSETS`) must be added to the live allowlist for the stablecoin bridge to run live (R4).

### Delivery / PR sequence

Each PR is independently mergeable/compilable/testable. `R#` = requirements covered.

1. **PR1 — Types & plumbing (no change for existing callers).** `freshness` field in `ic-xrc-types` + `.did` (with disclaimer); the dedicated `Other` error code in `errors.rs`; thread through `sanitize_request`, request log, dashboard column; add the cargo feature scaffold. Feature off ⇒ a `Live` request returns the error; omitting `freshness` is unchanged. Covers R1, R12 (off path), part of R14.
2. **PR2 — Flexible outcall binding.** `flexible_http.rs` + unit tests for arg construction, cycle calc, result/error decoding. Covers R11 (transform retained), binding for R3/R15.
3. **PR3 — Ticker endpoints.** Per-exchange ticker URL + parser (feature-gated) + fixtures + parser/URL tests. Settled untouched (R2).
4. **PR4 — Aggregation & spread.** Committee-median-per-exchange feeding the cross-exchange median; spread→`InconsistentRatesReceived`. Covers R3, R9.
5. **PR5 — Cache, coalescing, budget.** Live cache (T=10s, `fetched_at`), per-leg inflight (K=5), separate `LIVE_REQUEST_COUNTER_LIMIT`. Covers R6, R7, R8.
6. **PR6 — API routing.** `freshness` routing, allowlist gate, R10 timestamp, unchanged fee, no caller-identity special-casing. Covers R4, R5, R10, R15, R16 (confirm forex untouched).
7. **PR7 — Shadow-compute, metrics, alerts.** Shadow path + all new metrics + log/dashboard freshness; alert definitions handed to the monitoring repo. Covers R13, R14.
8. **Rollout (not a code PR):** beta soak → enable the feature in the production build via NNS upgrade → document as supported (still opt-in) after a clean monitored period.

**Pre-rollout gate (must pass before enabling the feature):** confirm — against the live registry, not assumed — that XRC's subnet (`uf6dk-hyaaa-aaaaq-qaaaq-cai`) has a `Free` cost schedule. This is load-bearing: on `Free`, flexible outcalls are free *and* available (legacy no-charge path); on `Normal`, the enable branch **rejects** flexible (PAYG unimplemented), so the feature would simply not work. Re-check for the `beta` canister's subnet too.

**Blocking dependency (separate ticket):** flexible-outcall test harness (PocketIC/replica divergent-response mocking) — unblocks divergent-path integration tests.

## Discussed Alternatives

- **Single-replica `is_replicated: false`** — rejected: one untrusted node defining an oracle price.
- **Flatten all node observations into one vector** — rejected: over-weights exchanges with more responding nodes; chose median-per-exchange.
- **Keep candle endpoints for `live`** (drop the 60s trim) — viable only for Coinbase/KuCoin; OKX/Bitget `history-candles` can't serve the forming minute. Chose uniform ticker endpoints.
- **Runtime kill-switch flag** — considered; user chose a compile-time feature instead (rebuild + upgrade to toggle).
- **Fallback to the last closed minute on high spread** — rejected (R9): return `InconsistentRatesReceived` instead, so callers are never silently handed a stale value labeled fresh.
- **Response provenance metadata** (rate_source / forming_minute flags) — deferred: the opt-in `freshness` field already tells the caller what it asked for.
- **True coalescing (second caller blocks and shares the first fetch's result)** — considered via a bounded yield-poll: the concurrent caller awaits a cheap self inter-canister call (a real syscall that resumes it), re-checks the cache, and loops until populated or a deadline. Rejected for v1: a canister call can only be resumed by its own outstanding syscall, so this needs extra intra-subnet round trips (~1–2s each) plus iteration/failure bounding, for marginal benefit over single-flight + `Pending` (which the T=10s cache already makes cheap and warm). Kept as a fallback if the retry round trip proves painful in the beta soak.
- **Server-side pinning of privileged callers to `settled`** — considered as a safety interlock (structurally prevent a forming-minute rate from ever reaching cycles-minting). Rejected: it contradicts the opt-in model, would silently override a caller that explicitly requested `live`, and is the only caller-identity special-case. The default (R1) plus the allowlist and beta feature gate already keep CMC/NNS on `settled`; choosing `settled` is the caller's responsibility.
- **Silently serving `Settled` when `Live` can't be provided** (non-allowlisted asset, feature off) — rejected as surprising: the caller explicitly opted in, so an explicit error is the honest contract (same reasoning as the R5 pin removal). The caller retries as `Settled` if it wants.
- **Silent production rollout without safeguards** — rejected: the disclaimer comment is not real protection; the allowlist, path isolation (separate budget/cache), compile-time gate, and live metrics/alerts are.
- **Bumping the settled `REQUEST_COUNTER_LIMIT` (56)** to share one budget — rejected in favor of a separate live counter so live and settled cannot starve each other.
