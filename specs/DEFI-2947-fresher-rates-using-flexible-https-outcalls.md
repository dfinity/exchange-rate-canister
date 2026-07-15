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
- **R4** — `Live` is served only for pairs built from the dedicated live allowlist `LIVE_ALLOWLIST = [BTC, ETH, ICP, USDC]` (each with deep forming-minute liquidity on 6+ exchanges), with `USDT` permitted **only as the bridge/quote asset**. Concretely, both the base and the quote must each be either in `LIVE_ALLOWLIST` or `USDT` (and not both `USDT`): this yields the single-leg pairs `{BTC,ETH,ICP,USDC}/USDT` plus the cross pairs among allowlisted assets (2 legs, e.g. `BTC/USDC`). Any other asset — **including all fiat** — makes a `Live` request return an **error** (`ExchangeRateError::Other` with a dedicated code), not a silent `Settled` downgrade. `LIVE_ALLOWLIST` is a **new constant** (a subset of `PRIVILEGED_CRYPTO_ASSETS`); it must **not** be expressed by mutating `PRIVILEGED_CRYPTO_ASSETS`, which feeds settled rate-limit-bypass logic (`utils::is_privileged_asset_pair`) that must stay unchanged (R2). Because fiat is never allowlisted, a `Live` pair never contains a fiat leg (a fiat conversion would still be settled — R16).
- **R5** — XRC does **not** special-case caller identity for `freshness`. Consensus-critical consumers (CMC/NNS) obtain `Settled` by not opting in (R1) and by their own judgment; it is the caller's choice. (Server-side pinning of privileged callers was considered and rejected — see Discussed Alternatives.)
- **R6** — `Live` outcalls draw on a **separate** in-flight outcall budget (`LIVE_REQUEST_COUNTER_LIMIT`), independent of the existing settled `REQUEST_COUNTER_LIMIT` (56, unchanged). Settled throughput is never reduced by live load, and vice versa.
- **R7** — `Live` fetch concurrency is bounded and de-duplicated. Coalescing granularity is the **leg** (`<symbol>/USDT`): at most **one** in-flight fetch per leg — a concurrent request for a leg already being fetched returns `Pending` (or a still-fresh cached value), never a duplicate outcall. Admission control granularity is the **pair**: at most `K = 5` concurrent distinct-pair fetches (each pair is 1–2 legs) within the live budget, beyond which a request returns `RateLimited`. The budget is sized for the generous case — 5 pairs × up to 2 legs × 9 exchanges ⇒ `LIVE_REQUEST_COUNTER_LIMIT ≈ 90` outcalls (R6).
- **R8** — `Live` rates are cached per asset with their observation time. A subsequent `Live` request for the same asset within `T = 10s` of the cached observation reuses the cached value and reports the **stored** observation time.
- **R9** — If the intra-exchange (committee) spread or the inter-exchange spread exceeds the configured thresholds, a `Live` request returns `InconsistentRatesReceived`. It does **not** silently fall back to a closed minute.
- **R10** — A `Live` response's `timestamp` is the **actual observation time**: `ic0.time()` captured after the outcalls resolve, **second-granular (not minute-floored)**. On a cache hit (R8), the stored observation time is returned.
- **R11** — The transform is retained in `Live` mode (each response reduced to the extracted rate) solely to minimize gossiped bytes; it is no longer relied on for determinism.
- **R12** — `Live` behavior is gated behind a **compile-time cargo feature**. When the feature is disabled, the `freshness` field is still accepted, but a `Live` request returns an **error** (`ExchangeRateError::Other`, same dedicated code as R4 with a distinct description) rather than a silent `Settled` downgrade. A request that omits `freshness` is unaffected (still `Settled`). Toggling live on/off is a rebuild + upgrade (this is the kill switch).
- **R13** — A **shadow-compute** path: when enabled, for a sampled fraction of `Settled` traffic the canister also computes the live rate **without returning it**, recording comparison metrics (R14). Shadow issues real flexible outcalls, so it produces data only where those work — a system subnet, in practice production — and is gated to the allowlist. Enablement and sample rate are **compile-time constants** (changed only by rebuild + NNS proposal; XRC has no out-of-band control).
- **R14** — New Prometheus metrics are exposed on the existing `/metrics` query (see Implementation), and the privileged/non-privileged request logs and dashboard carry the `freshness` of each request.
- **R15** — Flexible outcalls cost XRC nothing on its `Free`-cost-schedule subnet, and the caller-facing `get_exchange_rate` cycles fee is unchanged. On a `Free` subnet the enablement branch routes flexible requests through the **legacy no-charge** pricing path (`get_own_cost_schedule() == Free ⇒ PricingVersion::Legacy`), and the current `ic`-repo `master` PAYG path likewise short-circuits `charge()` when `Free` — free either way. (`get_own_cost_schedule`, `PricingVersion`, and the PAYG path are management-canister/replica code in the **`dfinity/ic`** repo, not XRC — see Constraints.) Note the initial availability is **free-subnet-only**: on a `Normal` subnet the enable branch routes to PAYG, which is not yet implemented and is rejected downstream (so the failure mode there is "unavailable", never "unexpectedly charged").
- **R16** — Forex/fiat retrieval and all governance/CMC-facing behavior remain on classic replicated outcalls, unchanged.
- **R17** — Shadow-compute has a **self-recovering autonomous circuit breaker**: on sustained per-exchange ticker 429/403 it pauses shadow outcalls to that exchange, and on a correlated rise in `settled` outcall failures it pauses shadow globally; when the triggering condition clears for a sustained period it **automatically resumes**, using hysteresis (distinct trip/reset thresholds + a minimum dwell time) to avoid flapping. Its state lives in replicated canister state — deterministic across replicas, since all execute over the same consensus-delivered flexible-response vector — and is exposed as a metric.

## Non-goals

- **No change to `settled`** outputs or code paths in this effort (a later consolidation of `settled` onto flexible outcalls, once `live` is proven, is explicitly out of scope — tracked separately).
- **Forex/fiat stays replicated** (daily cadence, no freshness need).
- **No new response provenance metadata** beyond the `timestamp` semantics of R10; a `live` caller already knows it opted in. The existing `standard_deviation` / `num_received_rates` convey quality.
- **`Live` is not suitable for consensus-critical use** (documented in the Candid comment). Consensus-critical consumers are expected to stay on `Settled`, but this is not enforced by caller identity (R5).
- **No runtime kill switch** — the kill switch is the compile-time feature (R12).
- **In-repo integration testing of divergent per-node responses is out of scope here.** The existing harness (single dfx replica + nginx; the in-flight PocketIC branch pins `additional_responses = vec![]`) cannot mock a committee returning differing responses. This is tracked as a **separate blocking dependency ticket** (PocketIC/replica flexible-outcall mocking). Until it lands, the divergent-path integration tests are blocked; confidence comes from unit tests + production shadow-compute (see Testing — `beta` cannot exercise flexible outcalls).
- `Live` is intentionally **not** offered for every asset (allowlist only, R4) nor in a feature-off build (R12); those cases return an explicit error rather than a silent `Settled` result. Omitting `freshness` always yields `Settled` and is never an error.
- **No non-production system-subnet test environment (initially).** Flexible outcalls are enabled on system subnets only for now (pricing undecided elsewhere), and `beta` is not on a system subnet — so real flexible-outcall testing happens via **production shadow-compute** (or a dedicated system-subnet test canister, if one can be provisioned with infra/NNS). `beta` still validates the non-flexible parts.

## Design Decisions

- **Flexible committee, not single-replica.** `Live` uses the new `flexible_http_request` (committee = full subnet), not the shipped single-replica `is_replicated: false` mode — a single untrusted node returning arbitrary prices is unacceptable for a price oracle. Full subnet keeps guarantees as close to today as possible; a flexible request consumes only **one** subnet in-flight slot regardless of committee size.
- **Committee quorum is a tunable compile-time constant, not a runtime knob.** `min_responses` (the fraction of committee nodes that must return an OK response before an exchange counts) is a named constant, adjustable only by rebuild + NNS proposal — consistent with XRC having no out-of-band runtime control (R12, and the shadow-compute constants). The default `min_responses = ⌈2N/3⌉+1` favors **trust** (hard to skew the median with a minority of nodes) at the cost of **availability**: it is precisely the popular, high-liquidity endpoints most likely to rate-limit (429/403) part of the committee, dropping OK responses below quorum and yielding an error. Lowering `min_responses` trades trust for availability. It stays a static constant (no dynamic adjustment) so behavior is deterministic across replicas and reviewable in a proposal; retune it from shadow data if the calm-market inconsistency rate is too high.
- **Median-per-exchange, then median-across-exchanges** (one vote per exchange), rather than flattening all node observations into one vector (which over-weights exchanges with more responding nodes). The committee spread per exchange becomes a quality signal.
- **Ticker endpoints for `live` only.** `Live` uses each exchange's real-time last-trade ticker endpoint; `settled` keeps its candle/OHLC endpoints unchanged. Uniform ticker use avoids per-exchange candle quirks (OKX/Bitget `history-candles` cannot serve the forming minute anyway).
- **Opt-in `freshness`, default `settled`, caller decides.** Backward compatible; CMC/NNS are unaffected because they don't opt in — not because of a server-side pin (R5).
- **Separate live budget + per-leg coalescing + `T = 10s` cache.** Because a cross-subnet call already takes ~10–15s, a ≤10s-old rate is near the practical freshness floor; the cache collapses bursts to one fetch per asset per 10s and is the main protection for the outcall budget.
- **Second-granular observation time** (R10) — honest freshness reporting; differs from settled's minute granularity.
- **Compile-time feature gate** as the kill switch (R12), consistent with existing `ipv4-support` / `application-subnet` feature style; toggling requires rebuild + NNS upgrade.
- **Shadow-compute control is proposal-gated — no out-of-band runtime knob.** XRC has no authorized principals and no controller-callable admin path; the only way to change behavior is an NNS upgrade proposal. Shadow's enable + sample rate are **compile-time constants** (change = rebuild + proposal); shadow is turned on via the enabling proposal once the replica feature is confirmed live on the subnet. Because there is no fast manual kill, instant safety comes from a **self-recovering autonomous circuit breaker** (R17): it backs off on detected harm and **automatically backs on** when the situation clears, with hysteresis to avoid flapping; a deliberate disable otherwise needs a (possibly expedited) proposal. The compile-time feature remains the ultimate presence gate for the public `live` path (R12).
- **Error, not fallback, on excessive spread** (R9).
- **Explicit errors, never silent downgrades.** A `Live` request that cannot be served as live (non-allowlisted asset R4, feature off R12) returns an error, not a `Settled` rate the caller didn't ask to receive. (Same reasoning that removed the R5 pin.)
- **Dedicated `LIVE_ALLOWLIST = [BTC, ETH, ICP, USDC]`** (a subset of `PRIVILEGED_CRYPTO_ASSETS`, priced against the `USDT` bridge), rather than reusing/mutating `PRIVILEGED_CRYPTO_ASSETS` directly: that set feeds settled rate-limit-bypass (`is_privileged_asset_pair`), so extending it (e.g. with `USDS`) would silently change settled behavior (R2). The four assets were chosen for deep, multi-exchange forming-minute liquidity; `USDS` and other thin stablecoin legs are deliberately excluded — their forming-minute tickers are frequently empty/stale, which in `live` mode would surface as chronic `InconsistentRatesReceived`.

## Implementation

### Constraints

- The replica `flexible_http_request` API — together with `get_own_cost_schedule`, `PricingVersion`, and the PAYG pricing path referenced in R15 — lives in the **`dfinity/ic`** repository (the management canister / replica), **not** in this repo. It is **not yet in the public `ic.did`**, currently sits on the **`eichhorl/*`** feature branch(es) there, and its field layout may still shift; it is expected to merge to `dfinity/ic` `master` the **week of 2026-07-20**. Build against the `eichhorl/*` branches first; switch the dependency to `master` once merged. Until then no build can be produced against `ic` `master`.
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

- Build `FlexibleCanisterHttpRequestArgs { url, headers, method: GET, body: None, transform, replication: Some(ReplicationCounts { total_requests = N, min_responses = MIN_COMMITTEE_RESPONSES, max_responses = N }) }`; `call_raw(management_canister, "flexible_http_request", …)`; decode `FlexibleHttpRequestResult`. `MIN_COMMITTEE_RESPONSES` (default `⌈2N/3⌉+1`) is a compile-time constant — the tunable trust/availability knob (see Design Decisions), changed only by rebuild + proposal.
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
- **Single-flight coalescing (chosen mechanism):** reuse the existing inflight set (keyed by `symbol` / `(symbol, LIVE)`). The first `Live` request for a leg sets the inflight flag and issues the one flexible fetch, populates the cache, then clears the flag. The check-then-set has no `await` between the two, so it is atomic — IC messages do not interleave except at `await` points, so there is no TOCTOU race. Concurrent requests for the same leg return `Pending` and retry onto the warm T=10s cache. The IC provides no way to resume a second caller on the *first* caller's outcall completion (a call resumes only on its own outstanding syscall), so blocking-and-sharing is deliberately not attempted (see Discussed Alternatives). Admit up to `K = 5` concurrent distinct-**pair** fetches (each 1–2 legs); beyond the live budget ⇒ `RateLimited`.
- **Trap-safety:** set the inflight flag via an RAII guard (like the existing `RateLimitingRequestCounterGuard`) so it clears on any early return. The flag is committed at the first `await` boundary, so a trap during response processing would otherwise wedge the leg; a failed/timed-out outcall returns normally and must run the cleanup.

### Rate limiting (`src/xrc/src/rate_limiting.rs`)

- New `LIVE_REQUEST_COUNTER_LIMIT` (~90, counted in outcalls: `K = 5` pairs × up to 2 legs × 9 exchanges), with its own RAII counter guard, separate from the settled 56. Admission is per-pair (R7); the budget is the generous 2-legs-per-pair sizing.

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
| `xrc_live_circuit_breaker_state{scope}` | gauge | 0 = active, 1 = backed off; `scope` ∈ per-exchange / global (R17) |

Add a `freshness` column to the request-log tables / dashboard.

Alerts (defined in the external k8s/monitoring repo, **beta severity** initially): live-vs-settled divergence; ticker 429/403 per exchange; flexible `too_many_rejects`/`timeout`; live-budget saturation; spurious inconsistency in calm markets; live staleness (mirror `IC_XRC_ExchangeSilent`); circuit-breaker back-off events (shadow paused, R17); and an **isolation guard** alert if settled `RateLimited`/error/latency regress after live ships.

### Feature gate (`src/xrc/Cargo.toml`, `scripts/build-wasm`, `Dockerfile`)

- New cargo feature (e.g. `live-rates`) gating all `live` behavior (R12); wire the build arg in `build-wasm`/`Dockerfile`. Since `beta` is not on a system subnet, the feature is compiled into the **production** build (initially dormant: public field unexposed, shadow disabled) and exercised there via shadow-compute — enabled by proposal once the replica feature is live on the subnet.

### Testing

- **Unit (bundle 1):** ticker parsers (fixtures); committee-median-per-exchange + spread→error (extend in-memory `QueriedExchangeRate` tests with per-node vectors); cache TTL/`fetched_at`; per-leg coalescing; separate budget; flexible error mapping; circuit-breaker trip/reset transitions + hysteresis (no flapping across the trip/reset band). With the public API (bundle 2): freshness routing; allowlist gating; feature-off ⇒ error; non-allowlisted ⇒ error; timestamp assignment.
- **Integration:** extend the mocked harness for the ticker URL/parse changes (single-response) now; the **divergent-path** tests are **blocked** on the harness dependency ticket.
- **No system-subnet test environment.** Flexible outcalls are system-subnet-only for now and `beta` is not on a system subnet, so `beta` validates only the non-flexible parts (settled regression, error paths, candid plumbing) — **not** real flexible outcalls. Real flexible validation therefore happens on a system subnet.
- **Shadow-compute — primary real-market validation.** On production (the only system-subnet deployment we have), once the replica feature is live on the subnet: records live-vs-settled divergence, intra-exchange spread, per-exchange flexible success, ticker HTTP status; no caller exposure; enable + sample rate are compile-time constants; a self-recovering autonomous circuit breaker backs off (and later resumes) on exchange rejections / settled regression (R17). Soak ~2–4 weeks spanning weekdays, ≥1 weekend, ≥1 volatility episode. If infra/NNS can provision a system-subnet test canister, run the soak there first.
- **`monitor-canister`** helps only once the public field exists and only against a system-subnet target (production): extend its pairs to the allowlist and request `live` to complement the shadow metrics.
- **Capacity load:** needs a system-subnet target; prefer bounding via shadow at natural load plus a small controlled burst (T=10s cache in the SUT), watching live-budget saturation and exchange 429/403 — a full-subnet committee on ticker endpoints is the main rate-limit risk.

### Edge cases

- All/most committee nodes reject or the exchange is down → `too_many_rejects` → error (R9-adjacent).
- Thin liquidity / very start of a minute → high spread → `InconsistentRatesReceived` (R9).
- Ticker endpoint rate-limits the committee (429/403) → counted as outcall failure; if OK responses fall below `min_responses`, error; metric `xrc_live_exchange_http_status` fires.
- Cache hit within T returns a stale-but-honest older `timestamp` (R8/R10).
- Non-allowlisted asset requested `live` → error (R4); feature disabled + `live` requested → error (R12); `freshness` omitted → `settled`, never an error. A privileged caller that opts into `live` for an allowlisted asset receives `live` — it is expected not to opt in (R5).
- Concurrent burst for the same asset leg → exactly one fetch; concurrent callers get `Pending` and retry onto the warm T=10s cache (R7).
- A `Live` request with a fiat leg (e.g. `BTC/USD`) or any non-allowlisted asset → error (R4); `USDT` is accepted only as the bridge/quote, never as a live-priced base asset on its own.

### Delivery / PR sequence

Two axes: **PRs** are merge units (each independently mergeable/compilable/testable); **deployments** are ~weekly NNS upgrade proposals for the production canister (`uf6dk-hyaaa-aaaaq-qaaaq-cai`), and several merged PRs bundle into one proposal. Sequenced so the **first deployment already carries shadow-compute + metrics** (data collection starts the moment the replica feature is live on the subnet) and the **public API change lands last** (after shadow data looks good). `R#` = requirements covered.

**Each PR below is a separate, independently-reviewed-and-merged PR** (they may stack, but each stands alone for review); a *bundle* is simply the set of already-merged PRs shipped together in one weekly deployment proposal. The bundle is not one big PR.

**Bundle 1 — internal live machinery + shadow + metrics (no public API change):**

1. **PR1 — Flexible outcall binding** — `flexible_http.rs` + unit tests (arg construction, cycle calc, result/error decoding). R11; binding for R3/R15.
2. **PR2 — Ticker endpoints** — per-exchange ticker URL + parser (feature-gated) + fixtures + parser/URL tests. Settled untouched (R2).
3. **PR3 — Aggregation & spread** — committee-median-per-exchange feeding the cross-exchange median; spread→`InconsistentRatesReceived`. R3, R9.
4. **PR4 — Cache, coalescing, budget** — live cache (T=10s, `fetched_at`), per-leg inflight (K=5), separate `LIVE_REQUEST_COUNTER_LIMIT`. R6, R7, R8.
5. **PR5 — Shadow-compute + metrics + circuit breaker** — compute `live` alongside sampled `settled` traffic without returning it; all new metrics + request-log/dashboard `freshness` column; enable + sample rate as compile-time constants; the self-recovering autonomous circuit breaker. No public API surface. R13, R14, R17.

→ **Deployment 1** (first weekly proposal): ships bundle 1 behind the compile-time feature, shadow disabled. A follow-up proposal enables shadow once the replica feature is confirmed live on XRC's subnet; begin the soak then.

**Bundle 2 — public exposure (only after the soak meets the exposure gate below):**

*Exposure gate — "the soak looks good" made concrete.* Bundle 2 ships only when, over the soak, for **every** allowlisted pair:

- live-vs-settled median relative difference stays within a small bound in calm markets (target on the order of a few bps; the exact threshold is finalized from the early shadow distribution, not assumed up front);
- the `InconsistentRatesReceived` rate in calm markets is below an agreed ceiling (spread thresholds are not chronically tripping);
- per-exchange flexible outcall success stays above an agreed floor, with ≥ `min_responses` OK responses the norm (no sustained 429/403 starvation);
- there is **no** correlated regression in settled `RateLimited`/error/latency (the isolation-guard alert never fires); and
- the circuit breaker (R17) is not chronically tripped for any exchange.

These are recorded as the go/no-go checklist for the exposure proposal; the exact numeric thresholds are pinned once the first ~week of shadow data establishes the calm-market baseline.


6. **PR6 — Public API + routing** — `freshness` field in `ic-xrc-types` + `.did` (with disclaimer) + the dedicated `Other` error code in `errors.rs`; `freshness` routing (allowlist gate R4, feature gate R12, R10 timestamp, unchanged fee R15, no caller-identity special-casing R5); thread `freshness` through `sanitize_request` / request log. R1, R4, R5, R10, R12, R16 (confirm forex untouched).

(PR6 may itself split — e.g. `6a` types + `.did` + error code, `6b` routing + threading — if that keeps each PR small; both must land in bundle 2.)

→ **Deployment 2…N**: adjust the shadow sample rate (a compile-time constant) by rebuild + proposal if needed during the ~2–4-week soak; a later weekly proposal ships bundle 2 to expose the field.

**Final:** document as supported (still opt-in) after a clean monitored period.

**Hard dependencies / gates:**

- **Replica enablement (critical path):** flexible outcalls must be enabled by the replica on XRC's *specific* production system subnet. Until then no real flexible outcall works anywhere available to us (`beta` is not on a system subnet), and shadow-compute cannot gather data.
- **Cost-schedule gate:** confirm — against the live registry, not assumed — that XRC's subnet (`uf6dk-hyaaa-aaaaq-qaaaq-cai`) is `Free` cost schedule. On `Free`, flexible is free *and* available (legacy no-charge path); on `Normal`, the enable branch **rejects** flexible (PAYG unimplemented), so the feature would not work.
- **Test-harness dependency (separate ticket):** flexible-outcall divergent-response mocking (PocketIC/replica) — unblocks divergent-path integration tests; until then divergence is only observable via production shadow-compute.

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
