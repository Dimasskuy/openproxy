//! Per-account consecutive-`invalid_grant` counter state.
//!
//! Lives in its own module so the rest of the provider can access the
//! counter without dragging in the retry loop / `OnUnhealthyCell`
//! implementation. Tests in `retry.rs` reach the `INVALID_GRANT_COUNTERS`
//! map via `super::super::counters::INVALID_GRANT_COUNTERS`.

use std::sync::LazyLock;
use std::sync::atomic::{AtomicU32, Ordering};

use dashmap::DashMap;

use crate::ids::AccountId;
use crate::oauth::DbRef;

/// Consecutive `invalid_grant` responses before marking the account `Unhealthy`.
/// Mirrors `UNHEALTHY_THRESHOLD` in `crate::oauth::mod`, kept independent because
/// this path runs on demand, not from the scheduler.
pub(crate) const ANTIGRAVITY_INVALID_GRANT_THRESHOLD: u32 = 3;

/// Backoff schedule (ms) between retries on `invalid_grant`.
/// `index 0 = before retry 1`, `index 1 = before retry 2`, `index 2 = before retry 3`.
pub(crate) const ANTIGRAVITY_BACKOFF_MS: [u64; 3] = [500, 1_000, 2_000];

/// Per-account consecutive-`invalid_grant` counter, process-scoped. A restart
/// resets it, which is fine: `accounts.health_status` is the source of truth for
/// blocked accounts.
///
/// Keyed by `account_id.0` so the hot path never builds a transient `String`,
/// and valued `AtomicU32` so concurrent refreshes of one account do not race.
pub(crate) static INVALID_GRANT_COUNTERS: LazyLock<DashMap<i64, AtomicU32>> =
    LazyLock::new(DashMap::new);

/// Maximum number of in-memory account failure counters permitted.
pub(crate) const MAX_INVALID_GRANT_ENTRIES: usize = 500;

/// Prune entries that have reached saturation (threshold) or evict excess entries
/// so that `INVALID_GRANT_COUNTERS` is strictly bounded to `MAX_INVALID_GRANT_ENTRIES`.
pub(crate) fn prune_invalid_grant_counters() -> usize {
    let mut pruned = 0;
    INVALID_GRANT_COUNTERS.retain(|_, count| {
        if count.load(Ordering::Relaxed) >= ANTIGRAVITY_INVALID_GRANT_THRESHOLD {
            pruned += 1;
            false
        } else {
            true
        }
    });
    while INVALID_GRANT_COUNTERS.len() >= MAX_INVALID_GRANT_ENTRIES {
        let key_to_remove = INVALID_GRANT_COUNTERS.iter().next().map(|e| *e.key());
        if let Some(k) = key_to_remove {
            INVALID_GRANT_COUNTERS.remove(&k);
            pruned += 1;
        } else {
            break;
        }
    }
    pruned
}

/// Increment the consecutive-`invalid_grant` counter for this account
/// and return its new value. Lock-free at the call site (the entry
/// insertion only happens on the first failure for a given account).
///
/// The counter is bounded: once it reaches `ANTIGRAVITY_INVALID_GRANT_THRESHOLD`,
/// subsequent bumps saturate at that value rather than growing without
/// limit (BUG-1 in `docs/specs/adversarial-findings.md`). `fetch_update`
/// guarantees the read-modify-write is atomic across threads, so
/// concurrent bumps for the same account converge on
/// `threshold`, not `N × threshold` (BUG-2).
pub(crate) fn bump(account_id: AccountId) -> u32 {
    if !INVALID_GRANT_COUNTERS.contains_key(&account_id.0)
        && INVALID_GRANT_COUNTERS.len() >= MAX_INVALID_GRANT_ENTRIES
    {
        prune_invalid_grant_counters();
    }
    let counter = INVALID_GRANT_COUNTERS
        .entry(account_id.0)
        .or_insert_with(|| AtomicU32::new(0));
    // `fetch_update` is a CAS loop returning the pre-update value, so the new
    // value is `previous + 1` unless the closure left it at the threshold. The
    // cap makes concurrent bumps converge on the threshold (BUG-2) and keeps
    // repeated calls from inflating it (BUG-1).
    let previous = counter
        .value()
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            if current >= ANTIGRAVITY_INVALID_GRANT_THRESHOLD {
                Some(current)
            } else {
                Some(current + 1)
            }
        })
        // the closure never returns `None`, so this branch is unreachable
        .unwrap_or(ANTIGRAVITY_INVALID_GRANT_THRESHOLD);
    // `previous` is the value the closure saw: bumped means `previous + 1`
    if previous >= ANTIGRAVITY_INVALID_GRANT_THRESHOLD {
        previous
    } else {
        previous + 1
    }
}

/// Reset the counter after a successful refresh by dropping the entry, so the
/// map only tracks accounts in a bad streak.
pub(crate) fn reset(account_id: AccountId) {
    INVALID_GRANT_COUNTERS.remove(&account_id.0);
}

/// Mark the account `Unhealthy` in the DB.
///
/// `DbRef::Pool` goes through a fire-and-forget `spawn_blocking` so the
/// synchronous write never stalls the runtime and the original `invalid_grant`
/// error reaches the caller unencumbered. `DbRef::Connection` locks the mutex
/// inline, which is the test path where no `DbPool` exists.
///
/// Failures are logged, never propagated: a secondary DB error must not mask the
/// original refresh failure.
pub(crate) fn mark_account_unhealthy(db: DbRef<'_>, account_id: AccountId) {
    let log_failure = move |e: &crate::error::CoreError, path: &str| {
        tracing::warn!(
            account = account_id.0,
            error = %e,
            path = path,
            "antigravity oauth: failed to set health to unhealthy"
        );
    };
    match db {
        DbRef::Pool(pool) => {
            let pool = pool.clone();
            tokio::task::spawn_blocking(move || {
                let conn = pool.writer();
                if let Err(e) = crate::accounts::set_health(
                    &conn,
                    account_id,
                    crate::accounts::HealthStatus::Unhealthy,
                ) {
                    log_failure(&e, "spawn_blocking");
                }
            });
        }
        DbRef::Connection(mutex) => {
            let conn = mutex.lock();
            if let Err(e) = crate::accounts::set_health(
                &conn,
                account_id,
                crate::accounts::HealthStatus::Unhealthy,
            ) {
                log_failure(&e, "test_path");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_invalid_grant_counters_capacity_bounding() {
        let _guard = TEST_MUTEX.lock().unwrap();
        INVALID_GRANT_COUNTERS.clear();
        for i in 0..MAX_INVALID_GRANT_ENTRIES {
            bump(AccountId(i as i64));
        }
        assert_eq!(INVALID_GRANT_COUNTERS.len(), MAX_INVALID_GRANT_ENTRIES);

        bump(AccountId(0));
        bump(AccountId(0)); // count == 3 (threshold)

        // a new entry triggers pruning of the saturated ones
        bump(AccountId((MAX_INVALID_GRANT_ENTRIES + 1) as i64));
        assert!(INVALID_GRANT_COUNTERS.len() <= MAX_INVALID_GRANT_ENTRIES);

        INVALID_GRANT_COUNTERS.clear();
    }

    #[test]
    fn test_invalid_grant_counters_saturation_600_accounts() {
        let _guard = TEST_MUTEX.lock().unwrap();
        INVALID_GRANT_COUNTERS.clear();
        // 600 distinct accounts, each below threshold
        for i in 0..600 {
            bump(AccountId(i));
            assert!(
                INVALID_GRANT_COUNTERS.len() <= MAX_INVALID_GRANT_ENTRIES,
                "Capacity exceeded at index {}: len is {}",
                i,
                INVALID_GRANT_COUNTERS.len()
            );
        }
        assert_eq!(INVALID_GRANT_COUNTERS.len(), MAX_INVALID_GRANT_ENTRIES);
        INVALID_GRANT_COUNTERS.clear();
    }

    #[test]
    fn test_invalid_grant_counters_eviction_order_threshold_first() {
        let _guard = TEST_MUTEX.lock().unwrap();
        INVALID_GRANT_COUNTERS.clear();
        for i in 0..MAX_INVALID_GRANT_ENTRIES {
            bump(AccountId(i as i64));
        }
        assert_eq!(INVALID_GRANT_COUNTERS.len(), MAX_INVALID_GRANT_ENTRIES);

        // account 10 saturates to threshold
        bump(AccountId(10));
        bump(AccountId(10));
        assert_eq!(
            INVALID_GRANT_COUNTERS
                .get(&10)
                .unwrap()
                .load(Ordering::Relaxed),
            ANTIGRAVITY_INVALID_GRANT_THRESHOLD
        );

        assert_eq!(
            INVALID_GRANT_COUNTERS
                .get(&20)
                .unwrap()
                .load(Ordering::Relaxed),
            1
        );

        // a new account evicts the saturated one before an active one
        bump(AccountId(9999));
        assert!(INVALID_GRANT_COUNTERS.len() <= MAX_INVALID_GRANT_ENTRIES);
        assert!(
            !INVALID_GRANT_COUNTERS.contains_key(&10),
            "Saturated account 10 should have been evicted first"
        );
        assert!(
            INVALID_GRANT_COUNTERS.contains_key(&20),
            "Active account 20 should be retained"
        );
        assert!(
            INVALID_GRANT_COUNTERS.contains_key(&9999),
            "New account 9999 should be inserted"
        );

        INVALID_GRANT_COUNTERS.clear();
    }
}
