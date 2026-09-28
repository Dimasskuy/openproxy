use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

struct TestCounterService {
    count: Arc<AtomicUsize>,
}

impl BackgroundService for TestCounterService {
    fn name(&self) -> &'static str {
        "test_counter"
    }

    async fn run(&self, cancel: CancellationToken) {
        let mut tick = tokio::time::interval(Duration::from_millis(5));
        loop {
            tokio::select! {
                () = cancel.cancelled() => break,
                _ = tick.tick() => {
                    self.count.fetch_add(1, Ordering::SeqCst);
                }
            }
        }
    }
}

#[tokio::test]
async fn supervisor_spawns_and_shuts_down_service() {
    let supervisor = BackgroundSupervisor::new();
    let count = Arc::new(AtomicUsize::new(0));

    let handle = supervisor.spawn(TestCounterService {
        count: Arc::clone(&count),
    });

    // Let the service run for a short duration
    for _ in 0..20 {
        if count.load(Ordering::SeqCst) > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(count.load(Ordering::SeqCst) > 0);

    // Signal graceful shutdown
    supervisor.shutdown();

    // Ensure task completes
    handle.await.expect("join handle");
    let stopped_at = count.load(Ordering::SeqCst);

    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(count.load(Ordering::SeqCst), stopped_at);
}

/// Smoke test for [`BackfillService`]: a fresh empty DB should
/// still complete one backfill pass and populate `last_run` /
/// `last_result` so the admin UI can clear the "warming up" banner.
/// We use a 60s interval so the service only runs one pass during
/// the test and exits cleanly on shutdown.
#[tokio::test]
async fn backfill_service_completes_initial_pass_on_empty_db() {
    use openproxy_db::DbPool;

    let pool =
        Arc::new(DbPool::test_pool_with_prefix("openproxy-backfill-test").expect("open pool"));

    let status = Arc::new(parking_lot::RwLock::new(
        crate::state::BackfillStatus::default(),
    ));
    let supervisor = BackgroundSupervisor::new();
    let handle = supervisor.spawn(BackfillService {
        db_pool: Arc::clone(&pool),
        backfill_status: Arc::clone(&status),
        interval: Duration::from_secs(60),
    });

    // Poll the status for up to 5s waiting for the first pass to
    // finish. On an empty DB the backfill is fast (just seeding
    // built-in providers + the bootstrap key).
    let mut completed = false;
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if status.read().last_run.is_some() {
            completed = true;
            break;
        }
    }
    supervisor.shutdown();
    handle.await.expect("join handle");

    let s = status.read().clone();
    assert!(completed, "backfill pass did not complete; status={s:?}");
    assert_eq!(s.last_result.as_deref(), Some("ok"));
    assert!(!s.in_progress, "status still in_progress after pass");
}

#[tokio::test]
async fn memory_cleanup_service_prunes_abandoned_inflight_and_trims() {
    use openproxy_db::DbPool;
    use openproxy_types::usage::InflightAttempt;

    let pool =
        Arc::new(DbPool::test_pool_with_prefix("openproxy-mem-cleanup-test").expect("open pool"));
    let selection_registry = Arc::new(openproxy_types::SelectionRegistry::new());
    let circuit_breaker = openproxy_pipeline::circuit_breaker::CircuitBreakerRegistry::new(
        &openproxy_types::config::CircuitBreakerConfig {
            failure_threshold: 5,
            unhealthy_duration_ms: 60_000,
        },
    );
    let predictive_limiter = Arc::new(openproxy_pipeline::PredictiveRateLimiter::new());
    let api_key_cache = Arc::new(dashmap::DashMap::new());

    let service = MemoryCleanupService {
        db_pool: Arc::clone(&pool),
        selection_registry,
        circuit_breaker,
        predictive_limiter,
        api_key_cache,
    };

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let fresh_key = "test-fresh-attempt-key".to_string();
    let stale_key = "test-stale-attempt-key".to_string();

    let make_attempt = |key: &str, updated_at_ms: u64| InflightAttempt {
        attempt_key: key.to_string(),
        request_id: "req".to_string(),
        trace_id: "trace".to_string(),
        provider_id: "p".to_string(),
        upstream_model_id: "m".to_string(),
        started_at_ms: updated_at_ms,
        updated_at_ms,
        stage: "started".to_string(),
        stage_seq: 1,
        stage_rank: 0,
        elapsed_ms_at_event: 0,
        connect_ms: None,
        ttft_ms: None,
        status_code: None,
        terminal: false,
        terminal_kind: None,
        error: None,
        row_id: None,
        source: "proxy".to_string(),
        endpoint_kind: None,
    };

    openproxy_core::usage::INFLIGHT_REGISTRY
        .insert(fresh_key.clone(), make_attempt(&fresh_key, now_ms));
    openproxy_core::usage::INFLIGHT_REGISTRY.insert(
        stale_key.clone(),
        make_attempt(&stale_key, now_ms.saturating_sub(400_000)),
    );

    service.run_cleanup_pass().await;

    assert!(
        !openproxy_core::usage::INFLIGHT_REGISTRY.contains_key(&stale_key),
        "stale inflight attempt (>300s) should have been pruned"
    );
    assert!(
        openproxy_core::usage::INFLIGHT_REGISTRY.contains_key(&fresh_key),
        "fresh inflight attempt should have been retained"
    );

    // Clean up
    openproxy_core::usage::INFLIGHT_REGISTRY.remove(&fresh_key);
}
