//! Action lifecycle orchestration.

use std::sync::Arc;

use orbitrelay_protocol::{Action, Event, EventId};

use crate::{EventPipeline, HandlerRegistry, RuntimeContext, RuntimeError};

/// The protocol execution engine.
pub struct Runtime {
    registry: Arc<HandlerRegistry>,
    context: RuntimeContext,
    pipeline: Arc<dyn EventPipeline>,
}

impl Runtime {
    /// Creates a runtime from its handler registry, context, and event pipeline.
    #[must_use]
    pub fn new(
        registry: Arc<HandlerRegistry>,
        context: RuntimeContext,
        pipeline: Arc<dyn EventPipeline>,
    ) -> Self {
        Self {
            registry,
            context,
            pipeline,
        }
    }

    /// Executes the complete action lifecycle and returns dispatched events.
    pub async fn execute(&self, action: Action) -> Result<Vec<Event>, RuntimeError> {
        let handler = self.registry.get(action.action_type()).ok_or_else(|| {
            RuntimeError::HandlerNotFound {
                action_type: action.action_type().clone(),
            }
        })?;

        handler
            .validate(&action, &self.context)
            .await
            .map_err(|source| RuntimeError::ValidationFailed {
                action_id: action.id().clone(),
                source,
            })?;

        self.context
            .authorizer()
            .authorize(&action)
            .await
            .map_err(|source| RuntimeError::AuthorizationFailed {
                action_id: action.id().clone(),
                source,
            })?;

        let scope =
            handler
                .execution_scope(&action)
                .map_err(|source| RuntimeError::HandlerFailed {
                    action_id: action.id().clone(),
                    source,
                })?;

        let execution_lease = match scope {
            Some(scope) => {
                let coordinator = self.context.execution_coordinator().ok_or_else(|| {
                    RuntimeError::CoordinationUnavailable {
                        action_id: action.id().clone(),
                        scope: scope.clone(),
                    }
                })?;
                let lease = coordinator.acquire(&scope).await.map_err(|source| {
                    RuntimeError::CoordinationFailed {
                        action_id: action.id().clone(),
                        scope,
                        source,
                    }
                })?;
                Some(lease)
            }
            None => None,
        };

        let drafts = handler
            .handle(&action, &self.context)
            .await
            .map_err(|source| RuntimeError::HandlerFailed {
                action_id: action.id().clone(),
                source,
            })?;

        let events = drafts
            .into_iter()
            .map(|draft| {
                let (event_type, payload, metadata) = draft.into_parts();
                Event::new(
                    EventId::new(),
                    action.session_id().clone(),
                    action.actor_id().clone(),
                    action.id().clone(),
                    event_type,
                    self.context.clock().now(),
                    payload,
                    metadata,
                )
            })
            .collect::<Vec<_>>();

        self.pipeline
            .dispatch(&events)
            .await
            .map_err(|source| RuntimeError::PipelineFailed {
                action_id: action.id().clone(),
                source,
            })?;

        drop(execution_lease);

        Ok(events)
    }

    /// Returns the dynamic handler registry.
    #[must_use]
    pub fn registry(&self) -> &HandlerRegistry {
        &self.registry
    }

    /// Returns the runtime dependency context.
    #[must_use]
    pub const fn context(&self) -> &RuntimeContext {
        &self.context
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    use async_trait::async_trait;
    use orbitrelay_core::{Metadata, Timestamp};
    use orbitrelay_protocol::{
        Action, ActionId, ActionType, ActorId, EventType, Payload, SessionId,
    };

    use super::Runtime;
    use crate::{
        ActionAuthorizer, ActionHandler, AllowAllAuthorizer, AuthorizationError, EventDraft,
        HandlerError, HandlerRegistry, MemoryEventPipeline, MockClock, RuntimeContext,
        RuntimeError,
    };

    struct TestHandler {
        handled: Arc<AtomicBool>,
        validation_error: Option<&'static str>,
        handler_error: Option<&'static str>,
    }

    #[async_trait]
    impl ActionHandler for TestHandler {
        async fn validate(
            &self,
            _action: &Action,
            _context: &RuntimeContext,
        ) -> Result<(), HandlerError> {
            match self.validation_error {
                Some(message) => Err(HandlerError::new(message)),
                None => Ok(()),
            }
        }

        async fn handle(
            &self,
            _action: &Action,
            _context: &RuntimeContext,
        ) -> Result<Vec<EventDraft>, HandlerError> {
            self.handled.store(true, Ordering::SeqCst);
            if let Some(message) = self.handler_error {
                return Err(HandlerError::new(message));
            }

            Ok(vec![EventDraft::new(
                EventType::new("canvas.drawn"),
                Payload::new(),
                Metadata::new(),
            )])
        }
    }

    struct RejectingAuthorizer;

    #[async_trait]
    impl ActionAuthorizer for RejectingAuthorizer {
        async fn authorize(&self, _action: &Action) -> Result<(), AuthorizationError> {
            Err(AuthorizationError::new("action denied"))
        }
    }

    fn action(action_type: &str, actor_id: ActorId, session_id: SessionId) -> Action {
        Action::new(
            ActionId::new(),
            session_id,
            actor_id,
            ActionType::new(action_type),
            Timestamp::from_unix_timestamp(1_600_000_000).expect("timestamp is valid"),
            Payload::new(),
            Metadata::new(),
        )
    }

    fn handler(handled: Arc<AtomicBool>) -> TestHandler {
        TestHandler {
            handled,
            validation_error: None,
            handler_error: None,
        }
    }

    #[tokio::test]
    async fn finds_handler_and_materializes_complete_event() {
        let registry = Arc::new(HandlerRegistry::new());
        let handled = Arc::new(AtomicBool::new(false));
        registry
            .register(
                ActionType::new("canvas.draw"),
                Arc::new(handler(handled.clone())),
            )
            .expect("handler registration should succeed");

        let occurred_at =
            Timestamp::from_unix_timestamp(1_700_000_000).expect("timestamp is valid");
        let context = RuntimeContext::new(
            Arc::new(MockClock::new(occurred_at.clone())),
            Arc::new(AllowAllAuthorizer),
        );
        let pipeline = Arc::new(MemoryEventPipeline::new());
        let runtime = Runtime::new(registry, context, pipeline.clone());
        let actor_id = ActorId::new();
        let session_id = SessionId::new();
        let action = action("canvas.draw", actor_id.clone(), session_id.clone());
        let action_id = action.id().clone();

        let events = runtime
            .execute(action)
            .await
            .expect("action should execute");

        assert!(handled.load(Ordering::SeqCst));
        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.actor_id(), &actor_id);
        assert_eq!(event.session_id(), &session_id);
        assert_eq!(event.action_id(), &action_id);
        assert_eq!(event.occurred_at(), &occurred_at);
        assert_eq!(event.event_type().as_str(), "canvas.drawn");
        assert_eq!(pipeline.events(), events);
    }

    #[tokio::test]
    async fn returns_error_when_handler_is_missing() {
        let runtime = Runtime::new(
            Arc::new(HandlerRegistry::new()),
            RuntimeContext::new(
                Arc::new(MockClock::new(Timestamp::now_utc())),
                Arc::new(AllowAllAuthorizer),
            ),
            Arc::new(MemoryEventPipeline::new()),
        );

        let error = runtime
            .execute(action("unknown.action", ActorId::new(), SessionId::new()))
            .await
            .expect_err("unregistered action should fail");

        assert!(matches!(error, RuntimeError::HandlerNotFound { .. }));
    }

    #[tokio::test]
    async fn stops_before_handler_when_authorization_is_rejected() {
        let registry = Arc::new(HandlerRegistry::new());
        let handled = Arc::new(AtomicBool::new(false));
        registry
            .register(
                ActionType::new("canvas.draw"),
                Arc::new(handler(handled.clone())),
            )
            .expect("handler registration should succeed");
        let pipeline = Arc::new(MemoryEventPipeline::new());
        let runtime = Runtime::new(
            registry,
            RuntimeContext::new(
                Arc::new(MockClock::new(Timestamp::now_utc())),
                Arc::new(RejectingAuthorizer),
            ),
            pipeline.clone(),
        );

        let error = runtime
            .execute(action("canvas.draw", ActorId::new(), SessionId::new()))
            .await
            .expect_err("rejected action should fail");

        assert!(matches!(error, RuntimeError::AuthorizationFailed { .. }));
        assert!(!handled.load(Ordering::SeqCst));
        assert!(pipeline.events().is_empty());
    }

    #[tokio::test]
    async fn reports_validation_failure_before_authorization() {
        let registry = Arc::new(HandlerRegistry::new());
        registry
            .register(
                ActionType::new("invalid.action"),
                Arc::new(TestHandler {
                    handled: Arc::new(AtomicBool::new(false)),
                    validation_error: Some("invalid action"),
                    handler_error: None,
                }),
            )
            .expect("handler registration should succeed");
        let runtime = Runtime::new(
            registry,
            RuntimeContext::new(
                Arc::new(MockClock::new(Timestamp::now_utc())),
                Arc::new(AllowAllAuthorizer),
            ),
            Arc::new(MemoryEventPipeline::new()),
        );

        let error = runtime
            .execute(action("invalid.action", ActorId::new(), SessionId::new()))
            .await
            .expect_err("invalid action should fail");

        assert!(matches!(error, RuntimeError::ValidationFailed { .. }));
    }
}

#[cfg(test)]
mod coordination_tests {
    use std::{
        collections::HashMap,
        sync::{
            atomic::{AtomicBool, AtomicUsize, Ordering},
            Arc, Mutex,
        },
        time::Duration,
    };

    use async_trait::async_trait;
    use orbitrelay_core::{Metadata, Timestamp};
    use orbitrelay_protocol::{
        Action, ActionId, ActionType, ActorId, Event, EventType, Payload, SessionId,
    };
    use tokio::{
        sync::{Barrier, Mutex as AsyncMutex, Notify, OwnedMutexGuard},
        time::timeout,
    };

    use super::Runtime;
    use crate::{
        ActionAuthorizer, ActionHandler, AllowAllAuthorizer, AuthorizationError, EventDraft,
        EventPipeline, ExecutionCoordinationError, ExecutionCoordinator, ExecutionLease,
        ExecutionScope, HandlerError, HandlerRegistry, MockClock, PipelineError, RuntimeContext,
        RuntimeError,
    };

    const TEST_TIMEOUT: Duration = Duration::from_secs(2);

    #[async_trait]
    trait HandleObserver: Send + Sync {
        async fn observe(&self);
    }

    struct NoopObserver;

    #[async_trait]
    impl HandleObserver for NoopObserver {
        async fn observe(&self) {}
    }

    struct FirstInvocationBlocker {
        calls: AtomicUsize,
        active: AtomicUsize,
        max_active: AtomicUsize,
        first_entered: Notify,
        release_first: Notify,
    }

    impl FirstInvocationBlocker {
        fn new() -> Self {
            Self {
                calls: AtomicUsize::new(0),
                active: AtomicUsize::new(0),
                max_active: AtomicUsize::new(0),
                first_entered: Notify::new(),
                release_first: Notify::new(),
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
            .expect("first handler should enter before timeout");
        }
    }

    #[async_trait]
    impl HandleObserver for FirstInvocationBlocker {
        async fn observe(&self) {
            let invocation = self.calls.fetch_add(1, Ordering::SeqCst);
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active.fetch_max(active, Ordering::SeqCst);

            if invocation == 0 {
                self.first_entered.notify_one();
                self.release_first.notified().await;
            }

            self.active.fetch_sub(1, Ordering::SeqCst);
        }
    }

    struct BarrierObserver {
        barrier: Arc<Barrier>,
        active: AtomicUsize,
        max_active: AtomicUsize,
    }

    impl BarrierObserver {
        fn new(barrier: Arc<Barrier>) -> Self {
            Self {
                barrier,
                active: AtomicUsize::new(0),
                max_active: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl HandleObserver for BarrierObserver {
        async fn observe(&self) {
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active.fetch_max(active, Ordering::SeqCst);
            self.barrier.wait().await;
            self.active.fetch_sub(1, Ordering::SeqCst);
        }
    }

    struct ScopedTestHandler {
        scope: Option<ExecutionScope>,
        handled: Arc<AtomicUsize>,
        observer: Arc<dyn HandleObserver>,
        fail: bool,
    }

    #[async_trait]
    impl ActionHandler for ScopedTestHandler {
        async fn validate(
            &self,
            _action: &Action,
            _context: &RuntimeContext,
        ) -> Result<(), HandlerError> {
            Ok(())
        }

        fn execution_scope(
            &self,
            _action: &Action,
        ) -> Result<Option<ExecutionScope>, HandlerError> {
            Ok(self.scope.clone())
        }

        async fn handle(
            &self,
            _action: &Action,
            _context: &RuntimeContext,
        ) -> Result<Vec<EventDraft>, HandlerError> {
            self.handled.fetch_add(1, Ordering::SeqCst);
            self.observer.observe().await;
            if self.fail {
                return Err(HandlerError::new("handler failed"));
            }

            Ok(vec![EventDraft::new(
                EventType::new("test.completed"),
                Payload::new(),
                Metadata::new(),
            )])
        }
    }

    struct TrackingLease {
        active_leases: Arc<AtomicUsize>,
        _guard: OwnedMutexGuard<()>,
    }

    impl ExecutionLease for TrackingLease {}

    impl Drop for TrackingLease {
        fn drop(&mut self) {
            self.active_leases.fetch_sub(1, Ordering::SeqCst);
        }
    }

    struct TrackingCoordinator {
        locks: Mutex<HashMap<ExecutionScope, Arc<AsyncMutex<()>>>>,
        acquire_calls: AtomicUsize,
        acquisition_started: Notify,
        active_leases: Arc<AtomicUsize>,
        fail: AtomicBool,
    }

    impl TrackingCoordinator {
        fn new() -> Self {
            Self {
                locks: Mutex::new(HashMap::new()),
                acquire_calls: AtomicUsize::new(0),
                acquisition_started: Notify::new(),
                active_leases: Arc::new(AtomicUsize::new(0)),
                fail: AtomicBool::new(false),
            }
        }

        async fn wait_for_acquire_calls(&self, expected: usize) {
            timeout(TEST_TIMEOUT, async {
                loop {
                    let notified = self.acquisition_started.notified();
                    if self.acquire_calls.load(Ordering::SeqCst) >= expected {
                        return;
                    }
                    notified.await;
                }
            })
            .await
            .expect("coordinator acquisition should start before timeout");
        }
    }

    #[async_trait]
    impl ExecutionCoordinator for TrackingCoordinator {
        async fn acquire(
            &self,
            scope: &ExecutionScope,
        ) -> Result<Box<dyn ExecutionLease>, ExecutionCoordinationError> {
            self.acquire_calls.fetch_add(1, Ordering::SeqCst);
            self.acquisition_started.notify_waiters();
            if self.fail.load(Ordering::SeqCst) {
                return Err(ExecutionCoordinationError::acquisition_failed(
                    "test acquisition failure",
                ));
            }

            let lock = self
                .locks
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .entry(scope.clone())
                .or_insert_with(|| Arc::new(AsyncMutex::new(())))
                .clone();
            let guard = lock.lock_owned().await;
            self.active_leases.fetch_add(1, Ordering::SeqCst);

            Ok(Box::new(TrackingLease {
                active_leases: self.active_leases.clone(),
                _guard: guard,
            }))
        }
    }

    struct TestPipeline {
        active_leases: Arc<AtomicUsize>,
        calls: AtomicUsize,
        alive_on_entry: AtomicBool,
        alive_before_return: AtomicBool,
        block: bool,
        fail: bool,
        entered: Notify,
        release: Notify,
    }

    impl TestPipeline {
        fn new(active_leases: Arc<AtomicUsize>) -> Self {
            Self {
                active_leases,
                calls: AtomicUsize::new(0),
                alive_on_entry: AtomicBool::new(false),
                alive_before_return: AtomicBool::new(false),
                block: false,
                fail: false,
                entered: Notify::new(),
                release: Notify::new(),
            }
        }

        async fn wait_for_entry(&self) {
            timeout(TEST_TIMEOUT, async {
                loop {
                    let notified = self.entered.notified();
                    if self.calls.load(Ordering::SeqCst) > 0 {
                        return;
                    }
                    notified.await;
                }
            })
            .await
            .expect("pipeline should be entered before timeout");
        }
    }

    #[async_trait]
    impl EventPipeline for TestPipeline {
        async fn dispatch(&self, _events: &[Event]) -> Result<(), PipelineError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.alive_on_entry.store(
                self.active_leases.load(Ordering::SeqCst) > 0,
                Ordering::SeqCst,
            );
            self.entered.notify_one();

            if self.block {
                self.release.notified().await;
            }

            self.alive_before_return.store(
                self.active_leases.load(Ordering::SeqCst) > 0,
                Ordering::SeqCst,
            );
            if self.fail {
                Err(PipelineError::new("pipeline failed"))
            } else {
                Ok(())
            }
        }
    }

    struct RejectingAuthorizer;

    #[async_trait]
    impl ActionAuthorizer for RejectingAuthorizer {
        async fn authorize(&self, _action: &Action) -> Result<(), AuthorizationError> {
            Err(AuthorizationError::new("action denied"))
        }
    }

    fn scope(key: &str) -> ExecutionScope {
        ExecutionScope::new("test.aggregate", key).expect("test scope should be valid")
    }

    fn action(action_type: &str) -> Action {
        Action::new(
            ActionId::new(),
            SessionId::new(),
            ActorId::new(),
            ActionType::new(action_type),
            Timestamp::from_unix_timestamp(1_700_000_000).expect("timestamp should be valid"),
            Payload::new(),
            Metadata::new(),
        )
    }

    fn handler(
        execution_scope: Option<ExecutionScope>,
        handled: Arc<AtomicUsize>,
        observer: Arc<dyn HandleObserver>,
        fail: bool,
    ) -> Arc<dyn ActionHandler> {
        Arc::new(ScopedTestHandler {
            scope: execution_scope,
            handled,
            observer,
            fail,
        })
    }

    fn runtime(
        registry: Arc<HandlerRegistry>,
        authorizer: Arc<dyn ActionAuthorizer>,
        pipeline: Arc<dyn EventPipeline>,
        coordinator: Option<Arc<dyn ExecutionCoordinator>>,
    ) -> Runtime {
        let mut context = RuntimeContext::new(
            Arc::new(MockClock::new(
                Timestamp::from_unix_timestamp(1_700_000_000).expect("timestamp should be valid"),
            )),
            authorizer,
        );
        if let Some(coordinator) = coordinator {
            context = context.with_execution_coordinator(coordinator);
        }
        Runtime::new(registry, context, pipeline)
    }

    fn register(registry: &HandlerRegistry, action_type: &str, handler: Arc<dyn ActionHandler>) {
        registry
            .register(ActionType::new(action_type), handler)
            .expect("test handler registration should succeed");
    }

    #[tokio::test]
    async fn unscoped_handler_runs_without_a_coordinator() {
        let registry = Arc::new(HandlerRegistry::new());
        let handled = Arc::new(AtomicUsize::new(0));
        register(
            &registry,
            "test.unscoped",
            handler(None, handled.clone(), Arc::new(NoopObserver), false),
        );
        let pipeline = Arc::new(TestPipeline::new(Arc::new(AtomicUsize::new(0))));
        let runtime = runtime(
            registry,
            Arc::new(AllowAllAuthorizer),
            pipeline.clone(),
            None,
        );

        let events = runtime
            .execute(action("test.unscoped"))
            .await
            .expect("unscoped action should remain compatible");

        assert_eq!(events.len(), 1);
        assert_eq!(handled.load(Ordering::SeqCst), 1);
        assert_eq!(pipeline.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn scoped_handler_without_coordinator_fails_closed() {
        let registry = Arc::new(HandlerRegistry::new());
        let handled = Arc::new(AtomicUsize::new(0));
        register(
            &registry,
            "test.scoped",
            handler(
                Some(scope("one")),
                handled.clone(),
                Arc::new(NoopObserver),
                false,
            ),
        );
        let pipeline = Arc::new(TestPipeline::new(Arc::new(AtomicUsize::new(0))));
        let runtime = runtime(
            registry,
            Arc::new(AllowAllAuthorizer),
            pipeline.clone(),
            None,
        );

        let error = runtime
            .execute(action("test.scoped"))
            .await
            .expect_err("scoped action must not run without coordination");

        assert!(matches!(
            error,
            RuntimeError::CoordinationUnavailable { .. }
        ));
        assert_eq!(handled.load(Ordering::SeqCst), 0);
        assert_eq!(pipeline.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn authorization_rejection_does_not_acquire_a_lease() {
        let registry = Arc::new(HandlerRegistry::new());
        let handled = Arc::new(AtomicUsize::new(0));
        register(
            &registry,
            "test.scoped",
            handler(
                Some(scope("one")),
                handled.clone(),
                Arc::new(NoopObserver),
                false,
            ),
        );
        let coordinator = Arc::new(TrackingCoordinator::new());
        let pipeline = Arc::new(TestPipeline::new(coordinator.active_leases.clone()));
        let runtime = runtime(
            registry,
            Arc::new(RejectingAuthorizer),
            pipeline.clone(),
            Some(coordinator.clone()),
        );

        let error = runtime
            .execute(action("test.scoped"))
            .await
            .expect_err("authorization should reject the action");

        assert!(matches!(error, RuntimeError::AuthorizationFailed { .. }));
        assert_eq!(coordinator.acquire_calls.load(Ordering::SeqCst), 0);
        assert_eq!(handled.load(Ordering::SeqCst), 0);
        assert_eq!(pipeline.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn lease_remains_alive_until_pipeline_dispatch_completes() {
        let registry = Arc::new(HandlerRegistry::new());
        let handled = Arc::new(AtomicUsize::new(0));
        register(
            &registry,
            "test.scoped",
            handler(Some(scope("one")), handled, Arc::new(NoopObserver), false),
        );
        let coordinator = Arc::new(TrackingCoordinator::new());
        let mut pipeline_value = TestPipeline::new(coordinator.active_leases.clone());
        pipeline_value.block = true;
        let pipeline = Arc::new(pipeline_value);
        let runtime = Arc::new(runtime(
            registry,
            Arc::new(AllowAllAuthorizer),
            pipeline.clone(),
            Some(coordinator.clone()),
        ));

        let execution = tokio::spawn({
            let runtime = runtime.clone();
            async move { runtime.execute(action("test.scoped")).await }
        });
        pipeline.wait_for_entry().await;

        assert!(pipeline.alive_on_entry.load(Ordering::SeqCst));
        assert_eq!(coordinator.active_leases.load(Ordering::SeqCst), 1);
        pipeline.release.notify_one();
        timeout(TEST_TIMEOUT, execution)
            .await
            .expect("execution should complete before timeout")
            .expect("execution task should not panic")
            .expect("action should succeed");

        assert!(pipeline.alive_before_return.load(Ordering::SeqCst));
        assert_eq!(coordinator.active_leases.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn handler_failure_releases_lease_without_dispatching() {
        let registry = Arc::new(HandlerRegistry::new());
        register(
            &registry,
            "test.scoped",
            handler(
                Some(scope("one")),
                Arc::new(AtomicUsize::new(0)),
                Arc::new(NoopObserver),
                true,
            ),
        );
        let coordinator = Arc::new(TrackingCoordinator::new());
        let pipeline = Arc::new(TestPipeline::new(coordinator.active_leases.clone()));
        let runtime = runtime(
            registry,
            Arc::new(AllowAllAuthorizer),
            pipeline.clone(),
            Some(coordinator.clone()),
        );

        let error = runtime
            .execute(action("test.scoped"))
            .await
            .expect_err("handler should fail");

        assert!(matches!(error, RuntimeError::HandlerFailed { .. }));
        assert_eq!(coordinator.active_leases.load(Ordering::SeqCst), 0);
        assert_eq!(pipeline.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn pipeline_failure_releases_lease() {
        let registry = Arc::new(HandlerRegistry::new());
        register(
            &registry,
            "test.scoped",
            handler(
                Some(scope("one")),
                Arc::new(AtomicUsize::new(0)),
                Arc::new(NoopObserver),
                false,
            ),
        );
        let coordinator = Arc::new(TrackingCoordinator::new());
        let mut pipeline_value = TestPipeline::new(coordinator.active_leases.clone());
        pipeline_value.fail = true;
        let pipeline = Arc::new(pipeline_value);
        let runtime = runtime(
            registry,
            Arc::new(AllowAllAuthorizer),
            pipeline,
            Some(coordinator.clone()),
        );

        let error = runtime
            .execute(action("test.scoped"))
            .await
            .expect_err("pipeline should fail");

        assert!(matches!(error, RuntimeError::PipelineFailed { .. }));
        assert_eq!(coordinator.active_leases.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn equal_scopes_enter_handlers_serially() {
        let registry = Arc::new(HandlerRegistry::new());
        let observer = Arc::new(FirstInvocationBlocker::new());
        register(
            &registry,
            "test.scoped",
            handler(
                Some(scope("same")),
                Arc::new(AtomicUsize::new(0)),
                observer.clone(),
                false,
            ),
        );
        let coordinator = Arc::new(TrackingCoordinator::new());
        let pipeline = Arc::new(TestPipeline::new(coordinator.active_leases.clone()));
        let runtime = Arc::new(runtime(
            registry,
            Arc::new(AllowAllAuthorizer),
            pipeline,
            Some(coordinator.clone()),
        ));

        let first = tokio::spawn({
            let runtime = runtime.clone();
            async move { runtime.execute(action("test.scoped")).await }
        });
        observer.wait_for_first_entry().await;
        let second = tokio::spawn({
            let runtime = runtime.clone();
            async move { runtime.execute(action("test.scoped")).await }
        });
        coordinator.wait_for_acquire_calls(2).await;

        assert_eq!(observer.calls.load(Ordering::SeqCst), 1);
        assert_eq!(observer.max_active.load(Ordering::SeqCst), 1);
        observer.release_first.notify_one();

        for execution in [first, second] {
            timeout(TEST_TIMEOUT, execution)
                .await
                .expect("execution should complete before timeout")
                .expect("execution task should not panic")
                .expect("action should succeed");
        }
        assert_eq!(observer.calls.load(Ordering::SeqCst), 2);
        assert_eq!(observer.max_active.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn different_scopes_enter_handlers_concurrently() {
        let registry = Arc::new(HandlerRegistry::new());
        let barrier = Arc::new(Barrier::new(3));
        let observer = Arc::new(BarrierObserver::new(barrier.clone()));
        register(
            &registry,
            "test.first",
            handler(
                Some(scope("first")),
                Arc::new(AtomicUsize::new(0)),
                observer.clone(),
                false,
            ),
        );
        register(
            &registry,
            "test.second",
            handler(
                Some(scope("second")),
                Arc::new(AtomicUsize::new(0)),
                observer.clone(),
                false,
            ),
        );
        let coordinator = Arc::new(TrackingCoordinator::new());
        let pipeline = Arc::new(TestPipeline::new(coordinator.active_leases.clone()));
        let runtime = Arc::new(runtime(
            registry,
            Arc::new(AllowAllAuthorizer),
            pipeline,
            Some(coordinator.clone()),
        ));

        let first = tokio::spawn({
            let runtime = runtime.clone();
            async move { runtime.execute(action("test.first")).await }
        });
        let second = tokio::spawn({
            let runtime = runtime.clone();
            async move { runtime.execute(action("test.second")).await }
        });
        timeout(TEST_TIMEOUT, barrier.wait())
            .await
            .expect("different scopes should reach handlers concurrently");

        for execution in [first, second] {
            timeout(TEST_TIMEOUT, execution)
                .await
                .expect("execution should complete before timeout")
                .expect("execution task should not panic")
                .expect("action should succeed");
        }
        assert_eq!(observer.max_active.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn acquisition_failure_stops_before_handler_and_pipeline() {
        let registry = Arc::new(HandlerRegistry::new());
        let handled = Arc::new(AtomicUsize::new(0));
        register(
            &registry,
            "test.scoped",
            handler(
                Some(scope("one")),
                handled.clone(),
                Arc::new(NoopObserver),
                false,
            ),
        );
        let coordinator = Arc::new(TrackingCoordinator::new());
        coordinator.fail.store(true, Ordering::SeqCst);
        let pipeline = Arc::new(TestPipeline::new(coordinator.active_leases.clone()));
        let runtime = runtime(
            registry,
            Arc::new(AllowAllAuthorizer),
            pipeline.clone(),
            Some(coordinator),
        );

        let error = runtime
            .execute(action("test.scoped"))
            .await
            .expect_err("lease acquisition should fail");

        assert!(matches!(error, RuntimeError::CoordinationFailed { .. }));
        assert_eq!(handled.load(Ordering::SeqCst), 0);
        assert_eq!(pipeline.calls.load(Ordering::SeqCst), 0);
    }
}
