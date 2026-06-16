//! Per-exchange discovered listings: the set of base assets each exchange
//! currently lists against USDT, refreshed on a timer and persisted across
//! upgrades. When an exchange has a fresh listing, the crypto path queries it
//! for an asset only if that listing contains the base; with no listing or a
//! stale one it fails open and queries anyway (see
//! [`ListingStore::should_query`]).
//!
//! A freshly fetched listing replaces the stored one only if it passes the
//! structural acceptance guard ([`ListingStore::accept`]); otherwise the
//! last-known-good listing is kept. The guard is judged on the TOTAL parsed
//! market count (across all quotes), never the USDT subset: a USDT->USD
//! migration collapses the USDT subset while leaving total markets roughly
//! unchanged, and must be accepted (so dead pairs stop being queried) rather
//! than rejected as if the response were broken.

use candid::{CandidType, Deserialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::exchanges::ListedPairs;

/// A refresh is rejected unless it parses to at least this many total markets.
/// Guards against a structurally valid but near-empty/garbage response.
pub(crate) const MIN_TOTAL_MARKETS: u64 = 50;

/// A refresh is rejected if its total market count falls below this fraction of
/// the previously accepted total — catching a parser break or truncated body,
/// while still accepting legitimate shrinkage (delistings, USDT->USD migration).
pub(crate) const MIN_RETAINED_FRACTION: f64 = 0.5;

/// A discovered listing older than this is treated as untrustworthy by the
/// gating read, which then queries the exchange for everything (fail-open)
/// rather than trusting a possibly-outdated set. With a daily refresh this
/// tolerates a few missed runs before failing open.
pub(crate) const MAX_LISTING_STALENESS_SECS: u64 = 3 * crate::ONE_DAY_SECONDS;

/// The last accepted listing for a single exchange.
///
/// This is persisted to stable memory via candid across upgrades, so it must
/// evolve compatibly: any field added later has to be `Option<T>` (candid
/// `opt`), otherwise decoding records persisted by an earlier version traps in
/// `post_upgrade`.
#[derive(CandidType, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ExchangeListing {
    /// Base assets tradable against USDT, uppercased and deduplicated. A
    /// `BTreeSet` so the gating read can test membership in O(log n) without
    /// maintaining a separate sorted-order invariant.
    pub bases: BTreeSet<String>,
    /// Total spot markets (across all quotes) in the last accepted refresh — the
    /// structural-health signal the acceptance guard is judged on.
    pub total_markets: u64,
    /// Timestamp (seconds) of the last accepted refresh.
    pub last_success_secs: u64,
}

/// Maps each exchange (by [`crate::Exchange::name`]) to its last accepted
/// listing. Persisted in stable memory across upgrades.
#[derive(CandidType, Deserialize, Clone, Debug, Default)]
pub(crate) struct ListingStore {
    by_exchange: BTreeMap<String, ExchangeListing>,
}

/// The result of offering a freshly fetched listing to the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AcceptOutcome {
    /// Stored as the new last-known-good listing.
    Accepted,
    /// Fewer than [`MIN_TOTAL_MARKETS`] markets parsed; previous listing kept.
    RejectedTooFewMarkets { total: u64 },
    /// Total markets dropped below [`MIN_RETAINED_FRACTION`] of the previous
    /// accepted total; previous listing kept.
    RejectedTotalDrop { total: u64, previous: u64 },
}

impl ListingStore {
    /// Offers a freshly fetched listing for `exchange` to the store. On a pass
    /// of the structural guard it becomes the new last-known-good listing and
    /// the function returns [`AcceptOutcome::Accepted`]; otherwise the existing
    /// listing (if any) is left untouched and a `Rejected*` variant is returned.
    pub(crate) fn accept(
        &mut self,
        exchange: &str,
        fetched: ListedPairs,
        now_secs: u64,
    ) -> AcceptOutcome {
        let total = fetched.total_markets as u64;

        if total < MIN_TOTAL_MARKETS {
            return AcceptOutcome::RejectedTooFewMarkets { total };
        }

        if let Some(previous) = self.by_exchange.get(exchange) {
            // Guard on TOTAL markets, not the USDT subset: a venue migrating
            // USDT->USD keeps total roughly stable (accepted) while its USDT
            // bases collapse, whereas a parser break collapses total (rejected).
            // Round the floor up: truncating would let an odd previous total
            // accept a refresh just under the retained fraction (e.g. previous
            // 101 -> floor 50, and 50/101 < 0.5 would slip through).
            let min_total = (previous.total_markets as f64 * MIN_RETAINED_FRACTION).ceil() as u64;
            if total < min_total {
                return AcceptOutcome::RejectedTotalDrop {
                    total,
                    previous: previous.total_markets,
                };
            }
        }

        self.by_exchange.insert(
            exchange.to_string(),
            ExchangeListing {
                bases: fetched.bases,
                total_markets: total,
                last_success_secs: now_secs,
            },
        );
        AcceptOutcome::Accepted
    }

    /// Returns the last accepted listing for `exchange`, if any.
    pub(crate) fn get(&self, exchange: &str) -> Option<&ExchangeListing> {
        self.by_exchange.get(exchange)
    }

    /// Whether the crypto path should query `exchange` for `base`/USDT.
    ///
    /// Fail-open: with no accepted listing for the exchange, or a listing older
    /// than [`MAX_LISTING_STALENESS_SECS`], the exchange is queried (`true`)
    /// rather than trusting a missing or stale set. Otherwise the exchange is
    /// queried only if its listing contains `base`. `base` is matched
    /// case-insensitively against the stored (uppercased) bases.
    pub(crate) fn should_query(&self, exchange: &str, base: &str, now_secs: u64) -> bool {
        match self.by_exchange.get(exchange) {
            None => true,
            Some(listing) => {
                let age = now_secs.saturating_sub(listing.last_success_secs);
                age > MAX_LISTING_STALENESS_SECS || listing.bases.contains(&base.to_uppercase())
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    /// Builds a set of base symbols from string literals.
    fn base_set(bases: &[&str]) -> BTreeSet<String> {
        bases.iter().map(|s| s.to_string()).collect()
    }

    /// Builds a [`ListedPairs`] from a base list and an explicit total.
    fn fetched(bases: &[&str], total_markets: usize) -> ListedPairs {
        ListedPairs {
            bases: base_set(bases),
            total_markets,
        }
    }

    /// Scenario 1/2: a valid refresh with no prior is accepted and stored, and a
    /// later single delisting is accepted without complaint.
    #[test]
    fn first_valid_refresh_is_accepted_and_stored() {
        let mut store = ListingStore::default();

        let outcome = store.accept("Okx", fetched(&["BTC", "ETH", "ICP"], 300), 1_000);
        assert_eq!(outcome, AcceptOutcome::Accepted);

        let listing = store.get("Okx").expect("listing should be stored");
        assert_eq!(listing.bases, base_set(&["BTC", "ETH", "ICP"]));
        assert_eq!(listing.total_markets, 300);
        assert_eq!(listing.last_success_secs, 1_000);

        // One pair delisted: total barely moves, accepted, base set shrinks.
        let outcome = store.accept("Okx", fetched(&["BTC", "ETH"], 299), 2_000);
        assert_eq!(outcome, AcceptOutcome::Accepted);
        assert_eq!(store.get("Okx").unwrap().bases, base_set(&["BTC", "ETH"]));
        assert_eq!(store.get("Okx").unwrap().last_success_secs, 2_000);
    }

    /// A structurally valid but near-empty response (below the absolute floor)
    /// is rejected; an existing good listing is kept.
    #[test]
    fn below_floor_is_rejected_and_keeps_previous() {
        let mut store = ListingStore::default();
        store.accept("GateIo", fetched(&["BTC", "ETH"], 2_000), 1_000);

        let outcome = store.accept("GateIo", fetched(&["BTC"], 3), 2_000);
        assert_eq!(outcome, AcceptOutcome::RejectedTooFewMarkets { total: 3 });

        // Previous listing untouched.
        let listing = store.get("GateIo").unwrap();
        assert_eq!(listing.total_markets, 2_000);
        assert_eq!(listing.last_success_secs, 1_000);
    }

    /// Scenario 4: a parser break / truncated body collapses the total markets;
    /// the drop below half the previous total is rejected and the last-good
    /// listing is kept.
    #[test]
    fn total_collapse_is_rejected_as_parser_break() {
        let mut store = ListingStore::default();
        store.accept("KuCoin", fetched(&["BTC", "ETH"], 1_000), 1_000);

        let outcome = store.accept("KuCoin", fetched(&["BTC", "ETH"], 100), 2_000);
        assert_eq!(
            outcome,
            AcceptOutcome::RejectedTotalDrop {
                total: 100,
                previous: 1_000
            }
        );
        assert_eq!(store.get("KuCoin").unwrap().total_markets, 1_000);
    }

    /// Scenario 3: a USDT->USD migration collapses the USDT subset while total
    /// markets stay roughly the same, so it is accepted (so the dead USDT pairs
    /// stop being queried) and the stored base set shrinks accordingly.
    #[test]
    fn usdt_to_usd_migration_is_accepted_even_as_bases_collapse() {
        let mut store = ListingStore::default();
        store.accept("Bitget", fetched(&["BTC", "ETH", "ICP"], 800), 1_000);

        // Same total markets, but (almost) none quoted in USDT anymore.
        let outcome = store.accept("Bitget", fetched(&[], 790), 2_000);
        assert_eq!(outcome, AcceptOutcome::Accepted);

        let listing = store.get("Bitget").unwrap();
        assert!(listing.bases.is_empty());
        assert_eq!(listing.total_markets, 790);
    }

    /// The absolute floor is inclusive: exactly [`MIN_TOTAL_MARKETS`] is
    /// accepted, one below is rejected.
    #[test]
    fn floor_boundary_is_inclusive() {
        let mut at_floor = ListingStore::default();
        assert_eq!(
            at_floor.accept("Mexc", fetched(&["BTC"], MIN_TOTAL_MARKETS as usize), 1),
            AcceptOutcome::Accepted
        );

        let mut below_floor = ListingStore::default();
        assert_eq!(
            below_floor.accept("Mexc", fetched(&["BTC"], (MIN_TOTAL_MARKETS - 1) as usize), 1),
            AcceptOutcome::RejectedTooFewMarkets {
                total: MIN_TOTAL_MARKETS - 1
            }
        );
    }

    /// The retained-fraction guard is inclusive: exactly half the previous total
    /// (and above the floor) is accepted, just below half is rejected. Uses a
    /// previous total large enough that half still clears the absolute floor.
    #[test]
    fn retained_fraction_boundary_is_inclusive() {
        let mut store = ListingStore::default();
        store.accept("Mexc", fetched(&["BTC"], 200), 1);

        // Exactly half of 200 is not below half -> accepted.
        assert_eq!(
            store.accept("Mexc", fetched(&["BTC"], 100), 2),
            AcceptOutcome::Accepted
        );

        // Restore the previous total to 200, then drop just below half.
        store.accept("Mexc", fetched(&["BTC"], 200), 3);
        assert_eq!(
            store.accept("Mexc", fetched(&["BTC"], 99), 4),
            AcceptOutcome::RejectedTotalDrop {
                total: 99,
                previous: 200
            }
        );
    }

    /// For an odd previous total the floor is rounded up, so a refresh strictly
    /// below half is rejected rather than slipping through a truncated floor
    /// (previous 101 -> floor 51, so 50/101 < 0.5 is rejected, 51/101 accepted).
    #[test]
    fn retained_fraction_floor_rounds_up_for_odd_previous() {
        let mut store = ListingStore::default();
        store.accept("Mexc", fetched(&["BTC"], 101), 1);
        assert_eq!(
            store.accept("Mexc", fetched(&["BTC"], 50), 2),
            AcceptOutcome::RejectedTotalDrop {
                total: 50,
                previous: 101
            }
        );

        store.accept("Mexc", fetched(&["BTC"], 101), 3);
        assert_eq!(
            store.accept("Mexc", fetched(&["BTC"], 51), 4),
            AcceptOutcome::Accepted
        );
    }

    /// With no accepted listing for an exchange, the gating read fails open.
    #[test]
    fn should_query_fails_open_without_a_listing() {
        let store = ListingStore::default();
        assert!(store.should_query("Okx", "ICP", 1_000));
    }

    /// A fresh listing gates by membership, matching the base case-insensitively.
    #[test]
    fn should_query_gates_by_membership_when_fresh() {
        let mut store = ListingStore::default();
        store.accept("Okx", fetched(&["BTC", "ICP"], 300), 1_000);

        assert!(store.should_query("Okx", "ICP", 1_000));
        assert!(store.should_query("Okx", "icp", 1_000));
        assert!(!store.should_query("Okx", "DOGE", 1_000));
    }

    /// A listing older than the staleness threshold fails open (queries
    /// everything) even for a base it does not contain.
    #[test]
    fn should_query_fails_open_when_stale() {
        let mut store = ListingStore::default();
        store.accept("Okx", fetched(&["BTC"], 300), 1_000);

        // Exactly at the threshold: not yet stale, so an absent base is skipped.
        assert!(!store.should_query("Okx", "DOGE", 1_000 + MAX_LISTING_STALENESS_SECS));
        // Just past the threshold: stale -> fail open.
        assert!(store.should_query("Okx", "DOGE", 1_000 + MAX_LISTING_STALENESS_SECS + 1));
    }
}
