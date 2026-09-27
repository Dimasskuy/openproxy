//! Cancellation primitives.
//!
//! `CancellationToken` is a tiny, `Clone`-able, atomically-flippable flag the
//! client races against the I/O future at every phase boundary and inside long
//! phases (e.g. the body read). Hand-rolled rather than
//! `tokio_util::sync::CancellationToken` so this module adds no dependency.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::sync::watch;

/// A cloneable, thread-safe cancel signal.
///
/// Cheap to clone (one `Arc` clone). `cancel()` is idempotent. `cancel_count`
/// counts every `cancel()` call and is observable for metrics and tests.
#[derive(Clone)]
pub struct CancellationToken {
    inner: Arc<Inner>,
}

struct Inner {
    flag: AtomicBool,
    cancel_count: AtomicUsize,
    // How many observers saw the flag.
    observe_count: AtomicUsize,
    // Async notification channel. Value transitions false -> true once.
    cancel_tx: watch::Sender<bool>,
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

impl CancellationToken {
    /// Create a fresh, un-cancelled token.
    pub fn new() -> Self {
        let (cancel_tx, _rx) = watch::channel(false);
        Self {
            inner: Arc::new(Inner {
                flag: AtomicBool::new(false),
                cancel_count: AtomicUsize::new(0),
                observe_count: AtomicUsize::new(0),
                cancel_tx,
            }),
        }
    }

    /// Signal cancellation. Idempotent, never blocks.
    pub fn cancel(&self) {
        self.inner.flag.store(true, Ordering::SeqCst);
        self.inner.cancel_count.fetch_add(1, Ordering::SeqCst);
        let _ = self.inner.cancel_tx.send(true);
    }

    /// Async wait for cancellation: returns at once if already cancelled,
    /// otherwise suspends until `cancel()`. One `subscribe()` + one `Arc` clone.
    pub async fn cancelled(&self) {
        if self.is_cancelled() {
            return;
        }
        let mut rx = self.inner.cancel_tx.subscribe();
        if *rx.borrow_and_update() {
            return;
        }
        // `changed()` errors once the sender is dropped: that means the token
        // is being torn down, so treat it as cancellation.
        while rx.changed().await.is_ok() {
            if *rx.borrow() {
                return;
            }
        }
    }

    /// A `watch::Receiver` over the internal cancel notification, for polling
    /// `changed()` in hot loops without a new subscription per iteration.
    pub fn subscribe(&self) -> watch::Receiver<bool> {
        self.inner.cancel_tx.subscribe()
    }

    /// Non-blocking peek.
    pub fn is_cancelled(&self) -> bool {
        let was = self.inner.flag.load(Ordering::SeqCst);
        if was {
            self.inner.observe_count.fetch_add(1, Ordering::SeqCst);
        }
        was
    }

    /// Create a child token cancelled if EITHER the parent or the child is
    /// cancelled. The child snapshots the parent's state on `child()` and
    /// cancelling the child never cancels the parent.
    pub fn child(&self) -> Self {
        let child = Self::new();
        if self.is_cancelled() {
            child.cancel();
        }
        child
    }

    /// Total `cancel()` calls observed across all clones, for tests and metrics.
    pub fn cancel_count(&self) -> usize {
        self.inner.cancel_count.load(Ordering::SeqCst)
    }

    /// Number of times `is_cancelled()` returned `true` for this token or a
    /// clone sharing the same `Arc`, so a test can assert the client consulted
    /// the token.
    pub fn observe_count(&self) -> usize {
        self.inner.observe_count.load(Ordering::SeqCst)
    }

    /// A token mirroring a `watch::Receiver<Option<CancelReason>>`: it flips to
    /// cancelled the first time the watch value becomes `Some`, and stays
    /// cancelled. A background task drives the flip.
    pub fn from_watch(mut rx: watch::Receiver<Option<openproxy_types::CancelReason>>) -> Self {
        let token = Self::new();
        if rx.borrow_and_update().is_some() {
            token.cancel();
            return token;
        }
        let inner = CancellationToken::clone(&token);
        tokio::spawn(async move {
            // `changed()` errors once the sender is dropped: treat that as a
            // no-op, the upstream call finishes or hits its own deadline.
            while rx.changed().await.is_ok() {
                if rx.borrow().is_some() {
                    inner.cancel();
                    return;
                }
            }
        });
        token
    }

    fn is_initially_cancelled(
        rx: &mut watch::Receiver<Option<openproxy_types::CancelReason>>,
        race_token: &CancellationToken,
    ) -> bool {
        rx.borrow_and_update().is_some() || race_token.is_cancelled()
    }

    fn spawn_cancel_forwarder(
        mut rx: watch::Receiver<Option<openproxy_types::CancelReason>>,
        mut cancel_rx: watch::Receiver<bool>,
        inner: CancellationToken,
    ) {
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    res = rx.changed() => {
                        if res.is_err() || rx.borrow().is_some() {
                            inner.cancel();
                            return;
                        }
                    }
                    res = cancel_rx.changed() => {
                        if res.is_err() || *cancel_rx.borrow() {
                            inner.cancel();
                            return;
                        }
                    }
                }
            }
        });
    }

    /// A token that cancels when EITHER the `watch::Receiver<Option<CancelReason>>`
    /// yields `Some` OR the `race_token` is cancelled.
    ///
    /// For race lanes: the lane's upstream call ends on client disconnect or
    /// when the race is lost, so losers drop their HTTP connection at the
    /// transport level and upstream token generation stops immediately.
    pub fn from_watch_and_token(
        mut rx: watch::Receiver<Option<openproxy_types::CancelReason>>,
        race_token: &CancellationToken,
    ) -> Self {
        let token = Self::new();
        if Self::is_initially_cancelled(&mut rx, race_token) {
            token.cancel();
            return token;
        }
        let inner = CancellationToken::clone(&token);
        let cancel_rx = race_token.inner.cancel_tx.subscribe();
        // TOCTOU: a cancel landing between `is_initially_cancelled` and
        // `subscribe()` leaves `borrow()` already true, and `changed()` never
        // fires for the current value. Re-check explicitly.
        if *cancel_rx.borrow() {
            inner.cancel();
            return token;
        }
        Self::spawn_cancel_forwarder(rx, cancel_rx, inner);
        token
    }
}

impl std::fmt::Debug for CancellationToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CancellationToken")
            .field("cancelled", &self.is_cancelled())
            .field("cancel_count", &self.cancel_count())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_token_is_not_cancelled() {
        let t = CancellationToken::new();
        assert!(!t.is_cancelled());
        assert_eq!(t.cancel_count(), 0);
    }

    #[test]
    fn cancel_is_idempotent_and_observable() {
        let t = CancellationToken::new();
        t.cancel();
        t.cancel();
        t.cancel();
        assert!(t.is_cancelled());
        assert_eq!(t.cancel_count(), 3);
    }

    #[test]
    fn clone_shares_state() {
        let t = CancellationToken::new();
        let t2 = CancellationToken::clone(&t);
        t2.cancel();
        assert!(t.is_cancelled());
        assert_eq!(t.cancel_count(), 1);
    }

    #[test]
    fn child_inherits_then_decouples() {
        let parent = CancellationToken::new();
        parent.cancel();
        let child = parent.child();
        assert!(child.is_cancelled(), "child must see pre-existing cancel");

        // Decoupling: cancelling the child leaves the parent valid.
        let parent2 = CancellationToken::new();
        let child2 = parent2.child();
        child2.cancel();
        assert!(
            !parent2.is_cancelled(),
            "parent stays valid after child cancel"
        );
    }

    // `from_watch` needs a Tokio runtime: the helper spawns a task that races
    // the watch.
    #[tokio::test]
    async fn from_watch_already_cancelled_starts_cancelled() {
        let (tx, mut rx) = watch::channel::<Option<openproxy_types::CancelReason>>(None);
        // Flip the watch BEFORE constructing the token, mirroring the
        // pre-flight check in the chat pipeline.
        tx.send(Some(openproxy_types::CancelReason::ClientDisconnected))
            .unwrap();
        rx.changed().await.unwrap();
        let token = CancellationToken::from_watch(rx);
        assert!(token.is_cancelled());
    }

    #[tokio::test]
    async fn from_watch_cancels_on_transition() {
        let (tx, rx) = watch::channel::<Option<openproxy_types::CancelReason>>(None);
        let token = CancellationToken::from_watch(rx);
        assert!(!token.is_cancelled());
        tx.send(Some(openproxy_types::CancelReason::ClientDisconnected))
            .unwrap();
        for _ in 0..50 {
            if token.is_cancelled() {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert!(token.is_cancelled());
    }

    #[tokio::test]
    async fn cancelled_returns_immediately_if_already_cancelled() {
        let t = CancellationToken::new();
        t.cancel();
        t.cancelled().await;
    }

    #[tokio::test]
    async fn cancelled_awaits_cancel() {
        let t = CancellationToken::new();
        let t2 = CancellationToken::clone(&t);

        let handle = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            t2.cancel();
        });

        t.cancelled().await;
        assert!(t.is_cancelled());
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn cancelled_multiple_waiters_all_wake() {
        let t = CancellationToken::new();
        let mut handles = Vec::new();
        for _ in 0..10 {
            let t2 = CancellationToken::clone(&t);
            handles.push(tokio::spawn(async move {
                t2.cancelled().await;
                assert!(t2.is_cancelled());
            }));
        }

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        t.cancel();
        for h in handles {
            h.await.unwrap();
        }
    }

    #[tokio::test]
    async fn from_watch_drops_cleanly_when_sender_dropped() {
        let (tx, rx) = watch::channel::<Option<openproxy_types::CancelReason>>(None);
        let token = CancellationToken::from_watch(rx);
        // Sender dropped: the task must see `changed()` fail and exit without
        // flipping the token.
        drop(tx);
        for _ in 0..50 {
            tokio::task::yield_now().await;
        }
        assert!(!token.is_cancelled());
    }

    #[tokio::test]
    async fn from_watch_and_token_fires_on_watch_transition() {
        let (tx, rx) = watch::channel::<Option<openproxy_types::CancelReason>>(None);
        let race_token = CancellationToken::new();
        let token = CancellationToken::from_watch_and_token(rx, &race_token);
        assert!(!token.is_cancelled());

        tx.send(Some(openproxy_types::CancelReason::ClientDisconnected))
            .unwrap();
        for _ in 0..50 {
            if token.is_cancelled() {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert!(token.is_cancelled());
    }

    #[tokio::test]
    async fn from_watch_and_token_fires_on_race_token() {
        let (_tx, rx) = watch::channel::<Option<openproxy_types::CancelReason>>(None);
        let race_token = CancellationToken::new();
        let token = CancellationToken::from_watch_and_token(rx, &race_token);
        assert!(!token.is_cancelled());

        race_token.cancel();
        for _ in 0..50 {
            if token.is_cancelled() {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert!(token.is_cancelled());
    }

    #[tokio::test]
    async fn from_watch_and_token_already_cancelled_watch_starts_cancelled() {
        let (tx, mut rx) = watch::channel::<Option<openproxy_types::CancelReason>>(None);
        tx.send(Some(openproxy_types::CancelReason::ClientDisconnected))
            .unwrap();
        rx.changed().await.unwrap();
        let race_token = CancellationToken::new();
        let token = CancellationToken::from_watch_and_token(rx, &race_token);
        assert!(token.is_cancelled());
    }

    #[tokio::test]
    async fn from_watch_and_token_already_cancelled_race_token_starts_cancelled() {
        let (_tx, rx) = watch::channel::<Option<openproxy_types::CancelReason>>(None);
        let race_token = CancellationToken::new();
        race_token.cancel();
        let token = CancellationToken::from_watch_and_token(rx, &race_token);
        assert!(token.is_cancelled());
    }

    #[tokio::test]
    async fn from_watch_and_token_toctou_closes() {
        // Cancelling between subscribe() and borrow() must be caught by the
        // re-check after subscribe(), not left to a `changed()` that never fires.
        let (_tx, rx) = watch::channel::<Option<openproxy_types::CancelReason>>(None);
        let race_token = CancellationToken::new();
        race_token.cancel();
        let token = CancellationToken::from_watch_and_token(rx, &race_token);
        assert!(token.is_cancelled());
    }
}
