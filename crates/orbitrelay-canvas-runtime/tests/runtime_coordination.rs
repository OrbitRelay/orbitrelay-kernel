mod common;

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use async_trait::async_trait;
use orbitrelay_canvas::{CanvasId, StrokeId, StrokeLifecycle, StrokeProjection};
use orbitrelay_canvas_runtime::{
    register_canvas_handlers, CanvasCommandService, CanvasStateReadError, CanvasStateReader,
};
use orbitrelay_core::Timestamp;
use orbitrelay_protocol::{Action, SessionId};
use orbitrelay_runtime::{
    ActionAuthorizer, AuthorizationError, Clock, ExecutionCoordinationError, ExecutionCoordinator,
    ExecutionLease, ExecutionScope, HandlerRegistry, MemoryEventPipeline, Runtime, RuntimeContext,
};
use tokio::{
    sync::{Mutex as AsyncMutex, Notify, OwnedMutexGuard},
    time::timeout,
};

use common::{Fixture, TestCanvasCatalog};

const TEST_TIMEOUT: Duration = Duration::from_secs(2);

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        Timestamp::from_unix_timestamp(1_700_000_000).expect("timestamp should be valid")
    }
}

struct AllowAuthorizer;

#[async_trait]
impl ActionAuthorizer for AllowAuthorizer {
    async fn authorize(&self, _action: &Action) -> Result<(), AuthorizationError> {
        Ok(())
    }
}

struct TestLease {
    _guard: OwnedMutexGuard<()>,
}

impl ExecutionLease for TestLease {}

struct TestKeyedCoordinator {
    locks: Mutex<HashMap<ExecutionScope, Arc<AsyncMutex<()>>>>,
    attempts: AtomicUsize,
    attempt_started: Notify,
}

impl TestKeyedCoordinator {
    fn new() -> Self {
        Self {
            locks: Mutex::new(HashMap::new()),
            attempts: AtomicUsize::new(0),
            attempt_started: Notify::new(),
        }
    }

    async fn wait_for_attempts(&self, expected: usize) {
        timeout(TEST_TIMEOUT, async {
            loop {
                let notified = self.attempt_started.notified();
                if self.attempts.load(Ordering::SeqCst) >= expected {
                    return;
                }
                notified.await;
            }
        })
        .await
        .expect("coordination attempts should start before timeout");
    }
}

#[async_trait]
impl ExecutionCoordinator for TestKeyedCoordinator {
    async fn acquire(
        &self,
        scope: &ExecutionScope,
    ) -> Result<Box<dyn ExecutionLease>, ExecutionCoordinationError> {
        self.attempts.fetch_add(1, Ordering::SeqCst);
        self.attempt_started.notify_waiters();
        let lock = self
            .locks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(scope.clone())
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
            .clone();
        Ok(Box::new(TestLease {
            _guard: lock.lock_owned().await,
        }))
    }
}

struct BlockingSequenceStateReader {
    calls: AtomicUsize,
    first_entered: Notify,
    release_first: Notify,
    existing: StrokeProjection,
}

impl BlockingSequenceStateReader {
    fn new(existing: StrokeProjection) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            first_entered: Notify::new(),
            release_first: Notify::new(),
            existing,
        }
    }

    async fn wait_for_first_entry(&self) {
        timeout(TEST_TIMEOUT, async {
            loop {
                let notified = self.first_entered.notified();
                if self.calls.load(Ordering::SeqCst) > 0 {
                    return;
                }
                notified.await;
            }
        })
        .await
        .expect("first state read should start before timeout");
    }
}

#[async_trait]
impl CanvasStateReader for BlockingSequenceStateReader {
    async fn load_stroke(
        &self,
        _session_id: &SessionId,
        _canvas_id: &CanvasId,
        _stroke_id: &StrokeId,
    ) -> Result<Option<StrokeProjection>, CanvasStateReadError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        if call == 0 {
            self.first_entered.notify_one();
            self.release_first.notified().await;
            Ok(None)
        } else {
            Ok(Some(self.existing.clone()))
        }
    }
}

#[tokio::test]
async fn same_stroke_canvas_actions_do_not_enter_state_handling_concurrently() {
    let fixture = Fixture::new();
    let state_reader = Arc::new(BlockingSequenceStateReader::new(
        fixture.projection(StrokeLifecycle::Active, false),
    ));
    let service = Arc::new(CanvasCommandService::new(
        Arc::new(TestCanvasCatalog::found(fixture.descriptor())),
        state_reader.clone(),
    ));
    let registry = Arc::new(HandlerRegistry::new());
    register_canvas_handlers(&registry, service).expect("handlers should register");
    let coordinator = Arc::new(TestKeyedCoordinator::new());
    let pipeline = Arc::new(MemoryEventPipeline::new());
    let runtime = Arc::new(Runtime::new(
        registry,
        RuntimeContext::new(Arc::new(FixedClock), Arc::new(AllowAuthorizer))
            .with_execution_coordinator(coordinator.clone()),
        pipeline.clone(),
    ));
    let payload = fixture.begin_payload();
    let first_action = fixture.action("canvas.stroke.begin", &payload);
    let second_action = fixture.action("canvas.stroke.begin", &payload);

    let first = tokio::spawn({
        let runtime = runtime.clone();
        async move { runtime.execute(first_action).await }
    });
    state_reader.wait_for_first_entry().await;
    let second = tokio::spawn({
        let runtime = runtime.clone();
        async move { runtime.execute(second_action).await }
    });
    coordinator.wait_for_attempts(2).await;

    assert_eq!(state_reader.calls.load(Ordering::SeqCst), 1);
    state_reader.release_first.notify_one();

    let first_events = timeout(TEST_TIMEOUT, first)
        .await
        .expect("first action should finish before timeout")
        .expect("first task should not panic")
        .expect("first action should succeed");
    let second_events = timeout(TEST_TIMEOUT, second)
        .await
        .expect("second action should finish before timeout")
        .expect("second task should not panic")
        .expect("second action should succeed");

    assert_eq!(first_events.len(), 1);
    assert!(second_events.is_empty());
    assert_eq!(state_reader.calls.load(Ordering::SeqCst), 2);
    assert_eq!(pipeline.events().len(), 1);
}
