//! Connection pool keyed by `(scheme, host, port)`.
//!
//! The spec asks for `Mutex<HashMap<HostKey, hyper::client::conn::http1::SendRequest>>`.
//! `SendRequest` is not `Clone` and owns its half of the connection, so holding
//! it in a shared `Mutex` would need `&mut` per send and serialize all traffic
//! to one host. `hyper_util::client::legacy::Client` is `Clone` and shares an
//! internal per-host pool, so the spec's surface (`UpstreamConnectionPool` is
//! `Clone` and exposes a `reuses()` counter) sits on top of it.
//!
//! A `PoolObserver` connector tracks reuse vs. fresh dial per host. The
//! counter is exposed through `UpstreamConnectionPool::reuses()` and is what
//! the `conn_pool_reuse` test asserts on.

use dashmap::DashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// `scheme://host:port` tuple (with optional proxy) that keys a pooled connection.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct HostKey {
    pub scheme: Scheme,
    pub host: String,
    pub port: u16,
    pub proxy: Option<String>,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum Scheme {
    Http,
    Https,
}

impl Scheme {
    pub fn from_uri(s: &str) -> Self {
        if s.eq_ignore_ascii_case("https") {
            Scheme::Https
        } else {
            Scheme::Http
        }
    }
}

impl HostKey {
    pub fn new(scheme: Scheme, host: impl Into<String>, port: u16) -> Self {
        Self {
            scheme,
            host: host.into(),
            port,
            proxy: None,
        }
    }

    pub fn with_proxy(
        scheme: Scheme,
        host: impl Into<String>,
        port: u16,
        proxy: Option<String>,
    ) -> Self {
        Self {
            scheme,
            host: host.into(),
            port,
            proxy,
        }
    }
}

/// Process-wide `Instant` captured at first use. Stored as millis in an
/// `AtomicU64` per pool entry to get a monotonic, lock-free "last used"
/// stamp: `Instant` is not storable in an atomic, so it converts on store and
/// load. Monotonicity comes from `Instant` (NTP-immune).
static PROCESS_START: std::sync::LazyLock<Instant> = std::sync::LazyLock::new(Instant::now);

fn process_start() -> Instant {
    *PROCESS_START
}

/// Monotonic millis since `process_start()`, stored in
/// `PoolEntry::last_used_ms`.
fn now_ms() -> u64 {
    Instant::now().duration_since(process_start()).as_millis() as u64
}

/// Per-host pool entry: the warm-connection hint plus observability counters.
/// Actual reuse happens in `hyper_util::client::legacy::Client`'s internal
/// pool.
#[derive(Debug)]
struct PoolEntry {
    /// Requests to this host that reused an already-open connection, so the
    /// second and later requests in a burst. The first is a dial.
    reuses: AtomicUsize,
    /// Total requests to this host, the denominator for `reuses`.
    total: AtomicUsize,
    /// Last successful use as monotonic millis since `process_start()`, read by
    /// the eviction sweep. Wall-clock rather than a tick count so idle hosts
    /// are evicted even when no traffic bumps the tick.
    last_used_ms: AtomicU64,
}

impl PoolEntry {
    fn new() -> Self {
        Self {
            reuses: AtomicUsize::new(0),
            total: AtomicUsize::new(0),
            last_used_ms: AtomicU64::new(now_ms()),
        }
    }
}

/// A shared, observable handle to a per-host connection pool.
///
/// Cloning is cheap (one `Arc` clone). Real sockets live in
/// `hyper_util::client::legacy::Client`; this struct holds the observability
/// counters and the idle-eviction sweep.
///
/// Idle eviction: a background sweep (started by `UpstreamClient::new`) wakes
/// every 30s and drops entries idle for more than 60s. The sweep uses
/// wall-clock `Instant` (monotonic, NTP-immune) so eviction does not depend on
/// request volume, which a request-bumped tick counter did: under low traffic
/// the tick barely advanced and idle entries survived. Since the legacy client
/// owns the sockets, evicting only prunes the observability map; the next
/// request to that host re-dials.
#[derive(Default)]
pub struct UpstreamConnectionPool {
    inner: Arc<DashMap<HostKey, PoolEntry>>,
}

impl Clone for UpstreamConnectionPool {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl UpstreamConnectionPool {
    pub fn new() -> Self {
        Self::default()
    }

    /// Total reuses across all hosts.
    pub fn reuses(&self) -> usize {
        self.inner
            .iter()
            .map(|e| e.value().reuses.load(Ordering::SeqCst))
            .sum()
    }

    /// Total requests across all hosts.
    pub fn total(&self) -> usize {
        self.inner
            .iter()
            .map(|e| e.value().total.load(Ordering::SeqCst))
            .sum()
    }

    /// Number of distinct hosts that have been seen at least once.
    pub fn host_count(&self) -> usize {
        self.inner.len()
    }

    /// Per-host reuses (for debugging / tests).
    pub fn reuses_for(&self, key: &HostKey) -> usize {
        self.inner
            .get(key)
            .map_or(0, |e| e.value().reuses.load(Ordering::SeqCst))
    }

    /// Check if the pool currently tracks an entry for `key`.
    pub fn contains_host(&self, key: &HostKey) -> bool {
        self.inner.contains_key(key)
    }

    /// Record a request to `key` on a freshly-dialed connection.
    pub fn record_dial(&self, key: HostKey) {
        let entry = self.inner.entry(key).or_insert_with(PoolEntry::new);
        entry.total.fetch_add(1, Ordering::Relaxed);
        entry.last_used_ms.store(now_ms(), Ordering::Relaxed);
    }

    /// Record a request to `key` that reused a pooled connection.
    pub fn record_reuse(&self, key: HostKey) {
        let entry = self.inner.entry(key).or_insert_with(PoolEntry::new);
        entry.total.fetch_add(1, Ordering::Relaxed);
        entry.reuses.fetch_add(1, Ordering::Relaxed);
        entry.last_used_ms.store(now_ms(), Ordering::Relaxed);
    }

    /// Drop entries idle for longer than `max_age` (wall-clock, not ticks).
    /// Returns how many were evicted.
    pub fn evict_older_than(&self, max_age: Duration) -> usize {
        let cutoff = now_ms().saturating_sub(max_age.as_millis() as u64);
        let mut evicted = 0;
        self.inner.retain(|_, e| {
            if e.last_used_ms.load(Ordering::SeqCst) >= cutoff {
                true
            } else {
                evicted += 1;
                false
            }
        });
        evicted
    }

    /// Spawn the background eviction loop holding only a `Weak` reference, so
    /// it exits once every strong `Arc` to the pool is dropped.
    pub fn spawn_eviction_loop(&self) {
        if tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        let weak_inner = Arc::downgrade(&self.inner);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            loop {
                interval.tick().await;
                let Some(inner) = weak_inner.upgrade() else {
                    break;
                };
                let cutoff = now_ms().saturating_sub(Duration::from_mins(1).as_millis() as u64);
                let mut evicted = 0;
                inner.retain(|_, e| {
                    if e.last_used_ms.load(Ordering::SeqCst) >= cutoff {
                        true
                    } else {
                        evicted += 1;
                        false
                    }
                });
                if evicted > 0 {
                    tracing::debug!(evicted, "upstream pool eviction sweep");
                }
            }
        });
    }
}

impl std::fmt::Debug for UpstreamConnectionPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpstreamConnectionPool")
            .field("host_count", &self.inner.len())
            .field(
                "reuses",
                &self
                    .inner
                    .iter()
                    .map(|e| e.value().reuses.load(Ordering::SeqCst))
                    .sum::<usize>(),
            )
            .field(
                "total",
                &self
                    .inner
                    .iter()
                    .map(|e| e.value().total.load(Ordering::SeqCst))
                    .sum::<usize>(),
            )
            .finish()
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_entry_creation() {
        let entry = PoolEntry::new();
        assert_eq!(entry.reuses.load(Ordering::SeqCst), 0);
        assert_eq!(entry.total.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_pool_record_dial() {
        let pool = UpstreamConnectionPool::new();
        let key = HostKey::new(Scheme::Https, "example.com", 443);
        pool.record_dial(key.clone());

        assert_eq!(pool.total(), 1);
        assert_eq!(pool.reuses(), 0);
        assert_eq!(pool.host_count(), 1);
        assert_eq!(pool.reuses_for(&key), 0);
    }

    #[test]
    fn test_pool_record_reuse() {
        let pool = UpstreamConnectionPool::new();
        let key = HostKey::new(Scheme::Https, "example.com", 443);
        pool.record_dial(key.clone());
        pool.record_reuse(key.clone());

        assert_eq!(pool.total(), 2);
        assert_eq!(pool.reuses(), 1);
        assert_eq!(pool.host_count(), 1);
        assert_eq!(pool.reuses_for(&key), 1);
    }

    #[test]
    fn test_pool_eviction() {
        let pool = UpstreamConnectionPool::new();
        let key1 = HostKey::new(Scheme::Https, "example.com", 443);
        let key2 = HostKey::new(Scheme::Http, "test.com", 80);

        pool.record_dial(key1);
        pool.record_dial(key2);

        assert_eq!(pool.host_count(), 2);

        let evicted = pool.evict_older_than(Duration::from_hours(1));
        assert_eq!(evicted, 0);
        assert_eq!(pool.host_count(), 2);

        std::thread::sleep(Duration::from_millis(5));

        let evicted = pool.evict_older_than(Duration::from_millis(0));
        assert_eq!(evicted, 2);
        assert_eq!(pool.host_count(), 0);
    }

    #[test]
    fn test_pool_proxy_isolation() {
        let pool = UpstreamConnectionPool::new();
        let key_direct = HostKey::new(Scheme::Https, "api.openai.com", 443);
        let key_p1 = HostKey::with_proxy(
            Scheme::Https,
            "api.openai.com",
            443,
            Some("http://proxy1.local:8080".into()),
        );
        let key_p2 = HostKey::with_proxy(
            Scheme::Https,
            "api.openai.com",
            443,
            Some("http://proxy2.local:8080".into()),
        );

        assert_ne!(key_direct, key_p1);
        assert_ne!(key_p1, key_p2);
        assert_ne!(key_direct, key_p2);

        pool.record_dial(key_direct.clone());
        pool.record_dial(key_p1.clone());
        pool.record_dial(key_p2.clone());

        assert_eq!(pool.host_count(), 3);
        assert!(pool.contains_host(&key_direct));
        assert!(pool.contains_host(&key_p1));
        assert!(pool.contains_host(&key_p2));

        pool.record_reuse(key_p1.clone());
        assert_eq!(pool.reuses_for(&key_direct), 0);
        assert_eq!(pool.reuses_for(&key_p1), 1);
        assert_eq!(pool.reuses_for(&key_p2), 0);
    }
}
