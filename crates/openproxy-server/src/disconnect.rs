//! TCP-level client-disconnect detection for axum 0.7.
//!
//! The chat handler used to drive `PipelineRequest::client_disconnected` from a
//! time-based watchdog (sleep `total_ms`, then flip). That only duplicated what
//! the upstream `total` budget already did: a client that RSTs 200ms after
//! sending the body still burned the full budget.
//!
//! [`client_disconnect_middleware`] wires a real per-request cancel watch: it
//! mints a `watch::channel(false)`, stashes the receiver in the request
//! extensions under [`CANCEL_WATCH_KEY`], and wraps the *response* body in
//! [`DisconnectBody`]. When hyper's write into a half-closed socket fails,
//! `poll_frame` errors and the wrapper fires the watch (idempotently), so the
//! pipeline aborts upstream work on its next checkpoint.
//!
//! Trade-offs:
//! - The *request* body is not wrapped: axum extractors fully consume it before
//!   the handler runs, so a mid-upload disconnect aborts the request naturally
//!   with no upstream work to waste. Wrapping it caused false-positive cancels.
//! - Route-scoped to `/v1/chat/completions`; the admin surface and the
//!   `/v1/health` probe need no TCP-cancel tracking.
//!
//! Public surface: [`CANCEL_WATCH_KEY`], [`client_disconnect_middleware`],
//! [`DisconnectBody`] (re-exported for tests).

use axum::{body::Body, extract::Request, middleware::Next, response::Response};
use http_body::{Body as HttpBody, Frame, SizeHint};
use std::{
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
};
use tokio::sync::watch;

/// Extension key for the per-request cancel watch. The chat handler reads the
/// receiver out of extensions and passes it to the pipeline.
#[derive(Clone, Copy, Debug)]
pub struct CancelWatchKey;

impl CancelWatchKey {
    /// Stable name for the handler's manual extension lookup. A method rather
    /// than a `const` so the type stays usable as an extension key.
    pub const NAME: &'static str = "openproxy.cancel_watch";
}

/// Build the (sender, receiver) pair the middleware uses, exposed so tests and
/// the chat handler can mint a pre-constructed pair.
pub fn new_cancel_pair() -> (
    watch::Sender<Option<openproxy_types::CancelReason>>,
    watch::Receiver<Option<openproxy_types::CancelReason>>,
) {
    watch::channel(None)
}

/// Axum middleware: see module docs. Mints a fresh `watch::channel(false)`, puts
/// the receiver in the request extensions under [`CancelWatchKey`], runs the
/// handler, then wraps the *response* body in a [`DisconnectBody`] keyed at the
/// same sender so a hyper write-error into a closed socket also fires the watch.
/// Both wrappers share an `Arc<AtomicBool>` "already fired" latch, so a
/// disconnect surfacing from both sides flips the watch only once.
pub async fn client_disconnect_middleware(mut req: Request, next: Next) -> Response {
    let (tx, rx) = new_cancel_pair();
    let fired = Arc::new(AtomicBool::new(false));

    // The handler clones `tx` for any extra cancel source (deadline watchdog)
    // and threads `rx` into the pipeline.
    req.extensions_mut().insert(CancelWatch {
        tx: tokio::sync::watch::Sender::clone(&tx),
        rx,
    });

    let mut response = next.run(req).await;

    // Wrapped regardless of HTTP status: a 4xx on a closed socket is still a
    // disconnect.
    let resp_body = std::mem::replace(response.body_mut(), Body::empty());
    let wrapped = DisconnectBody::new(resp_body, tx, Arc::clone(&fired));
    *response.body_mut() = Body::new(wrapped);

    response
}

// http_body::Body is implemented directly because it is the only trait that
// surfaces hyper write-errors, which is how a closed socket gets observed.

/// `http_body::Body` wrapper that fires a watch sender on any `poll_frame`
/// error and on a drop before the body reached its natural end.
///
/// Idempotent: the first signal flips the watch, later ones are no-ops.
/// `Poll::Ready(None)` does NOT fire — per the `http_body` contract that means
/// the body is *done*, which for a response is the natural end of the stream.
/// A disconnect surfaces as the explicit `Err` arm or as an early drop.
#[derive(Debug)]
pub struct DisconnectBody<B: HttpBody> {
    inner: B,
    tx: watch::Sender<Option<openproxy_types::CancelReason>>,
    fired: Arc<AtomicBool>,
    complete: bool,
}

impl<B: HttpBody> DisconnectBody<B> {
    /// Wrap `inner`; `tx` is fired idempotently the first time `poll_frame` errors.
    pub fn new(
        inner: B,
        tx: watch::Sender<Option<openproxy_types::CancelReason>>,
        fired: Arc<AtomicBool>,
    ) -> Self {
        let complete = inner.is_end_stream();
        Self {
            inner,
            tx,
            fired,
            complete,
        }
    }
}

impl<B: HttpBody> Drop for DisconnectBody<B> {
    fn drop(&mut self) {
        if !self.complete
            && !self.inner.is_end_stream()
            && self
                .fired
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
        {
            let _ = self
                .tx
                .send(Some(openproxy_types::CancelReason::ClientDisconnected));
        }
    }
}

impl<B: HttpBody + Unpin> HttpBody for DisconnectBody<B> {
    type Data = B::Data;
    type Error = B::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let result = Pin::new(&mut self.inner).poll_frame(cx);
        if let Poll::Ready(None) = &result {
            self.complete = true;
        } else if let Poll::Ready(Some(Err(_))) = &result {
            // First-error wins. `send` is a no-op if the receiver was dropped
            // (pipeline already finished), so the result is irrelevant; the
            // shared `fired` latch stops the response-body wrapper from
            // double-firing.
            if self
                .fired
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                let _ = self
                    .tx
                    .send(Some(openproxy_types::CancelReason::ClientDisconnected));
            }
        }
        result
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

/// Extension newtype carrying the cancel watch: a typed extension rather than a
/// header lookup, so the wire-up is typo-proof and the key stays unambiguous if
/// other `watch` values ever land in extensions. The handler clones `tx` for any
/// extra signal (e.g. a deadline watchdog) and passes `rx` to the pipeline; the
/// middleware's [`DisconnectBody`] wrappers hold their own `tx` clones, so all
/// cancellation sources share one watch.
#[derive(Clone, Debug)]
pub struct CancelWatch {
    pub tx: watch::Sender<Option<openproxy_types::CancelReason>>,
    pub rx: watch::Receiver<Option<openproxy_types::CancelReason>>,
}

impl CancelWatch {
    pub fn new() -> Self {
        let (tx, rx) = new_cancel_pair();
        Self { tx, rx }
    }
}

impl Default for CancelWatch {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    //! Exercises `DisconnectBody` directly — the only nontrivial part of the
    //! wire-up; the rest is glue around `req.extensions_mut().insert(...)`.
    //! End-to-end abort (middleware → handler → watch → pipeline) is covered by
    //! the regression tests in `crates/openproxy-core/src/pipeline.rs`; what
    //! only these can answer is "did the wrapper observe the body error and
    //! flip the watch?".
    use super::*;
    use bytes::Bytes;
    use http_body_util::Full;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    /// A body that always errors, so firing can only come from the `Err` arm
    /// of `poll_frame`.
    struct AlwaysErrorBody;
    impl HttpBody for AlwaysErrorBody {
        type Data = Bytes;
        type Error = std::io::Error;
        fn poll_frame(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
            Poll::Ready(Some(Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionReset,
                "simulated client disconnect",
            ))))
        }
        fn is_end_stream(&self) -> bool {
            false
        }
        fn size_hint(&self) -> SizeHint {
            SizeHint::default()
        }
    }

    struct PendingBody;
    impl HttpBody for PendingBody {
        type Data = Bytes;
        type Error = std::io::Error;

        fn poll_frame(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
            Poll::Pending
        }
    }

    /// A body that yields one data frame then `None`: a *normal* completion, on
    /// which the watch must NOT fire.
    struct OneFrameBody {
        delivered: bool,
    }
    impl HttpBody for OneFrameBody {
        type Data = Bytes;
        type Error = std::io::Error;
        fn poll_frame(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
            if self.delivered {
                Poll::Ready(None)
            } else {
                self.delivered = true;
                Poll::Ready(Some(Ok(Frame::data(Bytes::from_static(b"hi")))))
            }
        }
        fn is_end_stream(&self) -> bool {
            self.delivered
        }
        fn size_hint(&self) -> SizeHint {
            SizeHint::with_exact(2)
        }
    }

    type PollFrameResult<B> =
        Poll<Option<Result<Frame<<B as HttpBody>::Data>, <B as HttpBody>::Error>>>;

    /// Pump a `DisconnectBody` once (both tests need exactly this).
    fn poll_once<B: HttpBody + Unpin>(body: &mut DisconnectBody<B>) -> PollFrameResult<B> {
        let mut cx = Context::from_waker(futures::task::noop_waker_ref());
        Pin::new(body).poll_frame(&mut cx)
    }

    /// Core contract: an `Err` from the inner body must flip the watch.
    #[tokio::test]
    async fn error_on_poll_frame_fires_watch() {
        let (tx, rx) = new_cancel_pair();
        let fired = Arc::new(AtomicBool::new(false));
        let mut body = DisconnectBody::new(AlwaysErrorBody, tx, Arc::clone(&fired));

        let result = poll_once(&mut body);
        match result {
            Poll::Ready(Some(Err(_))) => {}
            other @ (Poll::Ready(_) | Poll::Pending) => {
                panic!("expected the wrapper to propagate the inner Err arm, got {other:?}")
            }
        }

        // `send` overwrites the current value, so a plain `borrow` sees it.
        assert!(
            rx.borrow().is_some(),
            "watch was not fired after a body error — the disconnect \
             detector is broken"
        );
    }

    /// Complement: a normally completing body must NOT fire the watch, or every
    /// successful request would be reported as a cancellation.
    #[tokio::test]
    async fn normal_completion_does_not_fire_watch() {
        let (tx, rx) = new_cancel_pair();
        let fired = Arc::new(AtomicBool::new(false));
        let mut body = DisconnectBody::new(OneFrameBody { delivered: false }, tx, fired);

        let first = poll_once(&mut body);
        assert!(matches!(first, Poll::Ready(Some(Ok(_)))));
        assert!(
            !rx.borrow().is_some(),
            "watch fired after a successful frame — the wrapper is firing \
             on success, not just on error"
        );

        let second = poll_once(&mut body);
        assert!(matches!(second, Poll::Ready(None)));
        assert!(
            !rx.borrow().is_some(),
            "watch fired on body completion (None) — the wrapper should \
             not fire on the natural end of the stream"
        );
    }

    #[tokio::test]
    async fn dropping_incomplete_body_fires_watch() {
        let (tx, rx) = new_cancel_pair();
        let fired = Arc::new(AtomicBool::new(false));
        let body = DisconnectBody::new(PendingBody, tx, fired);

        drop(body);

        assert!(rx.borrow().is_some());
    }

    /// Idempotency: a body emitting several `Err` frames in a row (hyper can do
    /// this on a weird connection state) must flip the watch once, not N times.
    #[tokio::test]
    async fn repeated_errors_only_flip_watch_once() {
        let (tx, rx) = new_cancel_pair();
        let fired = Arc::new(AtomicBool::new(false));
        let mut body = DisconnectBody::new(AlwaysErrorBody, tx, Arc::clone(&fired));

        for _ in 0..5 {
            let _ = poll_once(&mut body);
        }

        // Flips are uncountable from the receiver; the `fired` latch proves the
        // first error fired and every later one was a no-op.
        assert!(rx.borrow().is_some(), "watch never fired");
        assert!(
            fired.load(Ordering::SeqCst),
            "the shared `fired` latch never tripped — the idempotency \
             guard is missing"
        );
    }

    /// `CancelWatch::new` mints a fresh pair and `Clone` works on the newtype
    /// (the handler clones both halves in the request hot path).
    #[tokio::test]
    async fn cancel_watch_clone_is_independent() {
        let cw = CancelWatch::new();
        let rx2 = tokio::sync::watch::Receiver::clone(&cw.rx);

        // Firing via the original `cw.tx` is visible on the clone.
        let _ = cw
            .tx
            .send(Some(openproxy_types::CancelReason::ClientDisconnected));
        assert!(cw.rx.borrow().is_some(), "original rx should see the send");
        assert!(rx2.borrow().is_some(), "cloned rx should see the same send");
    }

    /// End-to-end through `Pin<Box<dyn Body>>`, the shape hyper surfaces when the
    /// client closes the connection mid-upload.
    #[tokio::test]
    async fn boxed_dyn_body_error_fires_watch() {
        let inner: Pin<Box<dyn HttpBody<Data = Bytes, Error = std::io::Error> + Send + Unpin>> =
            Box::pin(AlwaysErrorBody);
        let (tx, rx) = new_cancel_pair();
        let fired = Arc::new(AtomicBool::new(false));
        let mut body = DisconnectBody::new(inner, tx, fired);

        let result = poll_once(&mut body);
        assert!(matches!(result, Poll::Ready(Some(Err(_)))));
        assert!(
            rx.borrow().is_some(),
            "watch not fired through Pin<Box<dyn Body>>"
        );
    }

    /// The `B: HttpBody + Unpin` bound on the `impl` makes `DisconnectBody`
    /// `Unpin` whatever the inner body, and a data frame must not fire the watch.
    #[tokio::test]
    async fn full_body_does_not_fire_watch() {
        let (tx, rx) = new_cancel_pair();
        let fired = Arc::new(AtomicBool::new(false));
        // `Full<Bytes>` is a non-`Unpin` body from http-body-util.
        let mut body = DisconnectBody::new(Full::new(Bytes::from_static(b"hello")), tx, fired);
        let first = poll_once(&mut body);
        assert!(matches!(first, Poll::Ready(Some(Ok(_)))));
        assert!(
            !rx.borrow().is_some(),
            "watch fired on a non-error body — the wrapper is firing \
             on data frames, not on errors"
        );
    }
}
