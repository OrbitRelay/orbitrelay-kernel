//! WebSocket lifecycle adapter for already-established streams.

mod action_worker;
mod coordinator;
mod error;
mod event_pump;
mod query_worker;
mod reader;
mod session;
mod writer;

pub use error::WebSocketAdapterError;
pub use session::{
    from_raw_client_socket, from_raw_server_socket, run_websocket_session, WebSocketAdapterConfig,
    WebSocketSession, WebSocketSessionDependencies,
};

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        time::Duration,
    };

    use async_trait::async_trait;
    use futures_util::{SinkExt, StreamExt};
    use orbitrelay_core::{Metadata, Timestamp, Version};
    use orbitrelay_protocol::{
        Action, ActionId, ActionType, ActorId, Event, EventId, EventType, MessageEnvelope,
        MessageId, MessageType, Payload, SessionId,
    };
    use orbitrelay_query::{
        QueryActorContext, QueryExecutor, QueryFailure, QueryFailureCode, QueryRequest,
        QueryResponse, QueryResult,
    };
    use tokio::{io::duplex, sync::Mutex};
    use tokio_tungstenite::tungstenite::Message;

    use super::{
        from_raw_client_socket, from_raw_server_socket, run_websocket_session,
        WebSocketAdapterConfig, WebSocketAdapterError, WebSocketSessionDependencies,
    };
    use crate::{
        codec::{test_decode_outbound, test_encode_inbound},
        ActionExecutor, ActorBinding, Authenticate, CompatibleVersionPolicy, ConnectionId,
        ConnectionMetadata, EventSource, EventSourceError, EventSourceFactory, ExactVersionPolicy,
        Hello, IdentityError, IdentityResolver, IdentitySource, InboundCredentials, InboundMessage,
        OutboundMessage, SubscriptionAuthorizer, SubscriptionRequest, TransportConfig,
        TransportConnection, TransportErrorCode, TransportSubscriptionId, CURRENT_PROTOCOL_VERSION,
        QUERY_PROTOCOL_VERSION,
    };

    struct TestIdentityResolver {
        actor_id: ActorId,
        reject: bool,
    }

    #[async_trait]
    impl IdentityResolver for TestIdentityResolver {
        async fn resolve(
            &self,
            _connection_id: &ConnectionId,
            _credentials: &InboundCredentials,
        ) -> Result<ActorBinding, IdentityError> {
            if self.reject {
                return Err(IdentityError::CredentialsRejected {
                    detail: "test rejection".to_owned(),
                });
            }
            Ok(ActorBinding::new(
                self.actor_id.clone(),
                IdentitySource::new("test"),
            ))
        }
    }

    struct TestActionExecutor {
        calls: Arc<Mutex<Vec<ActionId>>>,
    }

    #[async_trait]
    impl ActionExecutor for TestActionExecutor {
        async fn execute(
            &self,
            action: Action,
        ) -> Result<Vec<Event>, crate::TransportExecutionError> {
            self.calls.lock().await.push(action.id().clone());
            Ok(vec![Event::new(
                EventId::new(),
                action.session_id().clone(),
                action.actor_id().clone(),
                action.id().clone(),
                EventType::new("test.completed"),
                Timestamp::from_unix_timestamp(1_700_000_002).expect("valid timestamp"),
                Payload::new(),
                Metadata::new(),
            )])
        }
    }

    struct ActiveQueryGuard {
        active: Arc<AtomicUsize>,
    }

    impl Drop for ActiveQueryGuard {
        fn drop(&mut self) {
            self.active.fetch_sub(1, Ordering::SeqCst);
        }
    }

    struct TestQueryExecutor {
        calls: Arc<AtomicUsize>,
        active: Arc<AtomicUsize>,
        max_active: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl QueryExecutor for TestQueryExecutor {
        async fn execute(&self, _actor: QueryActorContext, request: QueryRequest) -> QueryResponse {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active.fetch_max(active, Ordering::SeqCst);
            let _guard = ActiveQueryGuard {
                active: Arc::clone(&self.active),
            };
            let delay = request
                .payload()
                .get("delay_ms")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or_default();
            if delay > 0 {
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }
            if request.payload().get("fail").is_some() {
                return QueryResponse::error(
                    request.request_id().clone(),
                    request.query_type().clone(),
                    QueryFailure::new(QueryFailureCode::Internal, "failed", false),
                );
            }
            let mut result = Payload::new();
            if request.payload().get("large").is_some() {
                result.insert("data", serde_json::json!("x".repeat(4096)));
            }
            QueryResponse::success(
                request.request_id().clone(),
                request.query_type().clone(),
                result,
            )
        }
    }

    struct TestAuthorizer;

    #[async_trait]
    impl SubscriptionAuthorizer for TestAuthorizer {
        async fn authorize(
            &self,
            _binding: &ActorBinding,
            _request: &SubscriptionRequest,
        ) -> Result<(), crate::SubscriptionAuthorizationError> {
            Ok(())
        }
    }

    struct TestSource {
        id: TransportSubscriptionId,
        events: VecDeque<Event>,
    }

    #[async_trait]
    impl EventSource for TestSource {
        fn id(&self) -> &TransportSubscriptionId {
            &self.id
        }

        async fn next_event(&mut self) -> Result<Option<Event>, EventSourceError> {
            Ok(self.events.pop_front())
        }

        async fn close(&mut self) -> Result<(), EventSourceError> {
            self.events.clear();
            Ok(())
        }
    }

    struct TestSourceFactory {
        event: Option<Event>,
    }

    #[async_trait]
    impl EventSourceFactory for TestSourceFactory {
        async fn subscribe(
            &self,
            _request: SubscriptionRequest,
        ) -> Result<Box<dyn EventSource>, EventSourceError> {
            Ok(Box::new(TestSource {
                id: TransportSubscriptionId::new(),
                events: self.event.clone().into_iter().collect(),
            }))
        }
    }

    fn dependencies_with_rejection(
        actor_id: ActorId,
        event: Option<Event>,
        reject: bool,
    ) -> WebSocketSessionDependencies {
        WebSocketSessionDependencies::with_json_codec(
            Arc::new(TestActionExecutor {
                calls: Arc::new(Mutex::new(Vec::new())),
            }),
            Arc::new(TestIdentityResolver { actor_id, reject }),
            Arc::new(TestAuthorizer),
            Arc::new(TestSourceFactory { event }),
            Arc::new(ExactVersionPolicy),
        )
    }

    fn dependencies_with_query(
        actor_id: ActorId,
        query_executor: Arc<dyn QueryExecutor>,
        version_policy: Arc<dyn crate::VersionPolicy>,
    ) -> WebSocketSessionDependencies {
        WebSocketSessionDependencies::with_json_codec(
            Arc::new(TestActionExecutor {
                calls: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            }),
            Arc::new(TestIdentityResolver {
                actor_id,
                reject: false,
            }),
            Arc::new(TestAuthorizer),
            Arc::new(TestSourceFactory { event: None }),
            version_policy,
        )
        .with_query_executor(query_executor)
    }

    async fn connected_pair_with_query(
        actor_id: ActorId,
        query_executor: Arc<dyn QueryExecutor>,
        version_policy: Arc<dyn crate::VersionPolicy>,
    ) -> (
        tokio_tungstenite::WebSocketStream<tokio::io::DuplexStream>,
        tokio::task::JoinHandle<Result<(), WebSocketAdapterError>>,
    ) {
        connected_pair_with_query_config(
            actor_id,
            query_executor,
            version_policy,
            WebSocketAdapterConfig::default(),
        )
        .await
    }

    async fn connected_pair_with_query_config(
        actor_id: ActorId,
        query_executor: Arc<dyn QueryExecutor>,
        version_policy: Arc<dyn crate::VersionPolicy>,
        config: WebSocketAdapterConfig,
    ) -> (
        tokio_tungstenite::WebSocketStream<tokio::io::DuplexStream>,
        tokio::task::JoinHandle<Result<(), WebSocketAdapterError>>,
    ) {
        let (client_io, server_io) = duplex(64 * 1024);
        let client = from_raw_client_socket(client_io).await;
        let server = from_raw_server_socket(server_io).await;
        let connection = TransportConnection::new(ConnectionId::new(), ConnectionMetadata::new());
        let task = tokio::spawn(run_websocket_session(
            server,
            connection,
            dependencies_with_query(actor_id, query_executor, version_policy),
            config,
        ));
        (client, task)
    }

    async fn connected_pair(
        actor_id: ActorId,
        event: Option<Event>,
    ) -> (
        tokio_tungstenite::WebSocketStream<tokio::io::DuplexStream>,
        tokio::task::JoinHandle<Result<(), WebSocketAdapterError>>,
    ) {
        connected_pair_with_rejection(actor_id, event, false).await
    }

    async fn connected_pair_with_rejection(
        actor_id: ActorId,
        event: Option<Event>,
        reject: bool,
    ) -> (
        tokio_tungstenite::WebSocketStream<tokio::io::DuplexStream>,
        tokio::task::JoinHandle<Result<(), WebSocketAdapterError>>,
    ) {
        let (client_io, server_io) = duplex(64 * 1024);
        let client = from_raw_client_socket(client_io).await;
        let server = from_raw_server_socket(server_io).await;
        let connection = TransportConnection::new(ConnectionId::new(), ConnectionMetadata::new());
        let task = tokio::spawn(run_websocket_session(
            server,
            connection,
            dependencies_with_rejection(actor_id, event, reject),
            WebSocketAdapterConfig::default(),
        ));
        (client, task)
    }

    fn action(actor_id: ActorId) -> Action {
        Action::new(
            ActionId::new(),
            SessionId::new(),
            actor_id,
            ActionType::new("canvas.draw"),
            Timestamp::from_unix_timestamp(1_700_000_000).expect("valid timestamp"),
            Payload::new(),
            Metadata::new(),
        )
    }

    fn hello() -> InboundMessage {
        InboundMessage::Hello(Hello::new(
            vec![CURRENT_PROTOCOL_VERSION],
            vec!["json".to_owned()],
        ))
    }

    fn hello_query() -> InboundMessage {
        InboundMessage::Hello(Hello::new(
            vec![QUERY_PROTOCOL_VERSION],
            vec!["json".to_owned()],
        ))
    }

    fn query(version: Version, request_id: MessageId, payload: Payload) -> InboundMessage {
        InboundMessage::Query(crate::QueryMessage::new(
            version,
            request_id,
            orbitrelay_query::QueryType::new("document.list").expect("query type"),
            payload,
        ))
    }

    fn authenticate() -> InboundMessage {
        InboundMessage::Authenticate(Authenticate::new(
            MessageId::new(),
            InboundCredentials::new("test", "credential"),
        ))
    }

    async fn send_inbound(
        client: &mut tokio_tungstenite::WebSocketStream<tokio::io::DuplexStream>,
        message: InboundMessage,
    ) {
        client
            .send(Message::Text(
                String::from_utf8(test_encode_inbound(&message))
                    .expect("test JSON is UTF-8")
                    .into(),
            ))
            .await
            .expect("client frame should send");
    }

    async fn next_outbound(
        client: &mut tokio_tungstenite::WebSocketStream<tokio::io::DuplexStream>,
    ) -> OutboundMessage {
        loop {
            let frame = tokio::time::timeout(Duration::from_secs(2), client.next())
                .await
                .expect("server should respond")
                .expect("server should send a frame")
                .expect("WebSocket frame should be valid");
            if let Message::Text(text) = frame {
                return test_decode_outbound(text.as_bytes());
            }
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn hello_authenticate_and_action_acknowledgement_work() {
        let actor_id = ActorId::new();
        let action = action(actor_id.clone());
        let action_id = action.id().clone();
        let (mut client, server_task) = connected_pair(actor_id.clone(), None).await;

        send_inbound(&mut client, hello()).await;
        assert!(matches!(
            next_outbound(&mut client).await,
            OutboundMessage::HelloAccepted(_)
        ));
        send_inbound(&mut client, authenticate()).await;
        send_inbound(
            &mut client,
            InboundMessage::Action(MessageEnvelope::new(
                CURRENT_PROTOCOL_VERSION,
                MessageId::new(),
                MessageType::new("action"),
                action,
            )),
        )
        .await;

        let acknowledgement = next_outbound(&mut client).await;
        match acknowledgement {
            OutboundMessage::ActionAcknowledgement(ack) => {
                assert_eq!(ack.action_id(), &action_id);
                assert_eq!(ack.generated_event_ids().len(), 1);
            }
            other => panic!("expected action acknowledgement, got {other:?}"),
        }
        drop(client);
        let _ = server_task.await.expect("session task should join");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn action_before_hello_is_rejected_without_immediate_disconnect() {
        let actor_id = ActorId::new();
        let (mut client, server_task) = connected_pair(actor_id.clone(), None).await;
        send_inbound(
            &mut client,
            InboundMessage::Action(MessageEnvelope::new(
                CURRENT_PROTOCOL_VERSION,
                MessageId::new(),
                MessageType::new("action"),
                action(actor_id),
            )),
        )
        .await;

        match next_outbound(&mut client).await {
            OutboundMessage::Error(error) => {
                assert_eq!(error.code(), TransportErrorCode::ConnectionNotReady);
            }
            other => panic!("expected not-ready error, got {other:?}"),
        }
        send_inbound(&mut client, hello()).await;
        assert!(matches!(
            next_outbound(&mut client).await,
            OutboundMessage::HelloAccepted(_)
        ));
        drop(client);
        let _ = server_task.await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn forged_actor_is_rejected() {
        let bound_actor = ActorId::new();
        let forged_actor = ActorId::new();
        let (mut client, server_task) = connected_pair(bound_actor, None).await;
        send_inbound(&mut client, hello()).await;
        let _ = next_outbound(&mut client).await;
        send_inbound(&mut client, authenticate()).await;
        send_inbound(
            &mut client,
            InboundMessage::Action(MessageEnvelope::new(
                CURRENT_PROTOCOL_VERSION,
                MessageId::new(),
                MessageType::new("action"),
                action(forged_actor),
            )),
        )
        .await;

        match next_outbound(&mut client).await {
            OutboundMessage::Error(error) => {
                assert_eq!(error.code(), TransportErrorCode::IdentityMismatch);
            }
            other => panic!("expected identity error, got {other:?}"),
        }
        drop(client);
        let _ = server_task.await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn authentication_failure_is_safe_and_retryable() {
        let actor_id = ActorId::new();
        let (mut client, server_task) = connected_pair_with_rejection(actor_id, None, true).await;
        send_inbound(&mut client, hello()).await;
        let _ = next_outbound(&mut client).await;
        send_inbound(&mut client, authenticate()).await;

        match next_outbound(&mut client).await {
            OutboundMessage::Error(error) => {
                assert_eq!(error.code(), TransportErrorCode::AuthenticationRequired);
            }
            other => panic!("expected authentication error, got {other:?}"),
        }
        drop(client);
        let _ = server_task.await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn binary_frame_is_rejected_and_session_closes() {
        let actor_id = ActorId::new();
        let (mut client, server_task) = connected_pair(actor_id, None).await;
        client
            .send(Message::Binary(vec![1, 2, 3].into()))
            .await
            .expect("binary frame should send");

        match next_outbound(&mut client).await {
            OutboundMessage::Error(error) => {
                assert_eq!(error.code(), TransportErrorCode::InvalidMessage);
            }
            other => panic!("expected invalid-message error, got {other:?}"),
        }
        drop(client);
        assert!(server_task
            .await
            .expect("session task should join")
            .is_err());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn native_ping_and_subscription_event_are_forwarded() {
        let actor_id = ActorId::new();
        let event = Event::new(
            EventId::new(),
            SessionId::new(),
            actor_id.clone(),
            ActionId::new(),
            EventType::new("test.event"),
            Timestamp::from_unix_timestamp(1_700_000_010).expect("valid timestamp"),
            Payload::new(),
            Metadata::new(),
        );
        let (mut client, server_task) = connected_pair(actor_id.clone(), Some(event.clone())).await;
        send_inbound(&mut client, hello()).await;
        let _ = next_outbound(&mut client).await;
        send_inbound(&mut client, authenticate()).await;

        client
            .send(Message::Ping(vec![1, 2, 3].into()))
            .await
            .expect("native ping should send");
        loop {
            let frame = tokio::time::timeout(Duration::from_secs(2), client.next())
                .await
                .expect("pong should arrive")
                .expect("server should respond")
                .expect("frame should be valid");
            if let Message::Pong(payload) = frame {
                assert_eq!(payload.as_ref(), &[1, 2, 3]);
                break;
            }
        }

        send_inbound(
            &mut client,
            InboundMessage::Subscribe(SubscriptionRequest::new(
                MessageId::new(),
                event.session_id().clone(),
                [event.event_type().clone()],
            )),
        )
        .await;
        let first = next_outbound(&mut client).await;
        assert!(matches!(
            first,
            OutboundMessage::SubscriptionAccepted(_) | OutboundMessage::Event(_)
        ));
        let second = next_outbound(&mut client).await;
        assert!(matches!(
            second,
            OutboundMessage::SubscriptionAccepted(_) | OutboundMessage::Event(_)
        ));
        drop(client);
        let _ = server_task.await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn incompatible_version_returns_error_and_closes() {
        let actor_id = ActorId::new();
        let (mut client, server_task) = connected_pair(actor_id, None).await;
        send_inbound(
            &mut client,
            InboundMessage::Hello(Hello::new(
                vec![Version::new(9, 9, 9)],
                vec!["json".to_owned()],
            )),
        )
        .await;
        match next_outbound(&mut client).await {
            OutboundMessage::Error(error) => {
                assert_eq!(error.code(), TransportErrorCode::UnsupportedVersion);
            }
            other => panic!("expected version error, got {other:?}"),
        }
        drop(client);
        assert!(server_task
            .await
            .expect("session task should join")
            .is_err());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn protocol_02_query_executes_and_preserves_request_identity() {
        let actor_id = ActorId::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let query_executor = Arc::new(TestQueryExecutor {
            calls: Arc::clone(&calls),
            active: Arc::new(AtomicUsize::new(0)),
            max_active: Arc::new(AtomicUsize::new(0)),
        });
        let (mut client, server_task) =
            connected_pair_with_query(actor_id, query_executor, Arc::new(CompatibleVersionPolicy))
                .await;
        send_inbound(&mut client, hello_query()).await;
        match next_outbound(&mut client).await {
            OutboundMessage::HelloAccepted(accepted) => {
                assert_eq!(accepted.selected_version(), QUERY_PROTOCOL_VERSION)
            }
            other => panic!("expected 0.2 hello, got {other:?}"),
        }
        send_inbound(&mut client, authenticate()).await;
        let request_id = MessageId::new();
        send_inbound(
            &mut client,
            query(QUERY_PROTOCOL_VERSION, request_id.clone(), Payload::new()),
        )
        .await;
        match next_outbound(&mut client).await {
            OutboundMessage::QueryResponse(response) => {
                assert_eq!(response.request_id(), &request_id);
                assert!(matches!(response.result(), QueryResult::Success(_)));
            }
            other => panic!("expected query response, got {other:?}"),
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        drop(client);
        let _ = server_task.await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn protocol_01_query_is_rejected_without_executor_call() {
        let calls = Arc::new(AtomicUsize::new(0));
        let query_executor = Arc::new(TestQueryExecutor {
            calls: Arc::clone(&calls),
            active: Arc::new(AtomicUsize::new(0)),
            max_active: Arc::new(AtomicUsize::new(0)),
        });
        let actor_id = ActorId::new();
        let (mut client_io, server_task) = connected_pair_with_query(
            actor_id,
            query_executor,
            Arc::new(crate::ExactVersionPolicy),
        )
        .await;
        send_inbound(&mut client_io, hello()).await;
        assert!(matches!(
            next_outbound(&mut client_io).await,
            OutboundMessage::HelloAccepted(_)
        ));
        send_inbound(&mut client_io, authenticate()).await;
        send_inbound(
            &mut client_io,
            query(CURRENT_PROTOCOL_VERSION, MessageId::new(), Payload::new()),
        )
        .await;
        match next_outbound(&mut client_io).await {
            OutboundMessage::Error(error) => {
                assert_eq!(error.code(), TransportErrorCode::UnsupportedVersion)
            }
            other => panic!("expected unsupported-version error, got {other:?}"),
        }
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        drop(client_io);
        let _ = server_task.await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn slow_query_does_not_block_ping_and_fast_response_can_arrive_first() {
        let calls = Arc::new(AtomicUsize::new(0));
        let query_executor = Arc::new(TestQueryExecutor {
            calls,
            active: Arc::new(AtomicUsize::new(0)),
            max_active: Arc::new(AtomicUsize::new(0)),
        });
        let (mut client, server_task) = connected_pair_with_query(
            ActorId::new(),
            query_executor,
            Arc::new(CompatibleVersionPolicy),
        )
        .await;
        send_inbound(&mut client, hello_query()).await;
        let _ = next_outbound(&mut client).await;
        send_inbound(&mut client, authenticate()).await;
        let slow_id = MessageId::new();
        let fast_id = MessageId::new();
        let mut slow_payload = Payload::new();
        slow_payload.insert("delay_ms", serde_json::json!(80));
        send_inbound(
            &mut client,
            query(QUERY_PROTOCOL_VERSION, slow_id.clone(), slow_payload),
        )
        .await;
        send_inbound(
            &mut client,
            query(QUERY_PROTOCOL_VERSION, fast_id.clone(), Payload::new()),
        )
        .await;
        send_inbound(
            &mut client,
            InboundMessage::Ping(crate::PingMessage::new(7)),
        )
        .await;

        let mut response_ids = Vec::new();
        let mut saw_pong = false;
        while response_ids.len() < 2 || !saw_pong {
            match next_outbound(&mut client).await {
                OutboundMessage::QueryResponse(response) => {
                    response_ids.push(response.request_id().clone())
                }
                OutboundMessage::Pong(pong) => saw_pong = pong.nonce() == 7,
                other => panic!("unexpected response: {other:?}"),
            }
        }
        assert!(saw_pong);
        assert_eq!(response_ids, vec![fast_id, slow_id]);
        drop(client);
        let _ = server_task.await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn disconnect_cancels_pending_query_and_releases_active_count() {
        let active = Arc::new(AtomicUsize::new(0));
        let query_executor = Arc::new(TestQueryExecutor {
            calls: Arc::new(AtomicUsize::new(0)),
            active: Arc::clone(&active),
            max_active: Arc::new(AtomicUsize::new(0)),
        });
        let (mut client, server_task) = connected_pair_with_query(
            ActorId::new(),
            query_executor,
            Arc::new(CompatibleVersionPolicy),
        )
        .await;
        send_inbound(&mut client, hello_query()).await;
        let _ = next_outbound(&mut client).await;
        send_inbound(&mut client, authenticate()).await;
        let mut payload = Payload::new();
        payload.insert("delay_ms", serde_json::json!(10_000));
        send_inbound(
            &mut client,
            query(QUERY_PROTOCOL_VERSION, MessageId::new(), payload),
        )
        .await;
        tokio::time::sleep(Duration::from_millis(20)).await;
        drop(client);
        let _ = tokio::time::timeout(Duration::from_secs(1), server_task)
            .await
            .expect("disconnect should stop query worker")
            .expect("session task should join");
        assert_eq!(active.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn query_concurrency_is_bounded_by_connection_config() {
        let max_active = Arc::new(AtomicUsize::new(0));
        let query_executor = Arc::new(TestQueryExecutor {
            calls: Arc::new(AtomicUsize::new(0)),
            active: Arc::new(AtomicUsize::new(0)),
            max_active: Arc::clone(&max_active),
        });
        let config = WebSocketAdapterConfig::default()
            .with_query_queue_capacity(8)
            .with_max_in_flight_queries(2);
        let (mut client, server_task) = connected_pair_with_query_config(
            ActorId::new(),
            query_executor,
            Arc::new(CompatibleVersionPolicy),
            config,
        )
        .await;
        send_inbound(&mut client, hello_query()).await;
        let _ = next_outbound(&mut client).await;
        send_inbound(&mut client, authenticate()).await;
        for _ in 0..4 {
            let mut payload = Payload::new();
            payload.insert("delay_ms", serde_json::json!(30));
            send_inbound(
                &mut client,
                query(QUERY_PROTOCOL_VERSION, MessageId::new(), payload),
            )
            .await;
        }
        let mut completed = 0;
        while completed < 4 {
            if matches!(
                next_outbound(&mut client).await,
                OutboundMessage::QueryResponse(_)
            ) {
                completed += 1;
            }
        }
        assert_eq!(max_active.load(Ordering::SeqCst), 2);
        drop(client);
        let _ = server_task.await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn slow_query_does_not_block_action_or_event_delivery() {
        let actor_id = ActorId::new();
        let session_id = SessionId::new();
        let action_calls = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let event = Event::new(
            EventId::new(),
            session_id.clone(),
            actor_id.clone(),
            ActionId::new(),
            EventType::new("test.event"),
            Timestamp::from_unix_timestamp(1_700_000_020).expect("timestamp"),
            Payload::new(),
            Metadata::new(),
        );
        let dependencies = WebSocketSessionDependencies::with_json_codec(
            Arc::new(TestActionExecutor {
                calls: Arc::clone(&action_calls),
            }),
            Arc::new(TestIdentityResolver {
                actor_id: actor_id.clone(),
                reject: false,
            }),
            Arc::new(TestAuthorizer),
            Arc::new(TestSourceFactory {
                event: Some(event.clone()),
            }),
            Arc::new(CompatibleVersionPolicy),
        )
        .with_query_executor(Arc::new(TestQueryExecutor {
            calls: Arc::new(AtomicUsize::new(0)),
            active: Arc::new(AtomicUsize::new(0)),
            max_active: Arc::new(AtomicUsize::new(0)),
        }));
        let (client_io, server_io) = duplex(64 * 1024);
        let mut client = from_raw_client_socket(client_io).await;
        let server = from_raw_server_socket(server_io).await;
        let server_task = tokio::spawn(run_websocket_session(
            server,
            TransportConnection::new(ConnectionId::new(), ConnectionMetadata::new()),
            dependencies,
            WebSocketAdapterConfig::default(),
        ));
        send_inbound(&mut client, hello_query()).await;
        let _ = next_outbound(&mut client).await;
        send_inbound(&mut client, authenticate()).await;

        let slow_id = MessageId::new();
        let mut payload = Payload::new();
        payload.insert("delay_ms", serde_json::json!(100));
        send_inbound(
            &mut client,
            query(QUERY_PROTOCOL_VERSION, slow_id.clone(), payload),
        )
        .await;
        send_inbound(
            &mut client,
            InboundMessage::Action(MessageEnvelope::new(
                QUERY_PROTOCOL_VERSION,
                MessageId::new(),
                MessageType::new("action"),
                action(actor_id),
            )),
        )
        .await;
        send_inbound(
            &mut client,
            InboundMessage::Subscribe(SubscriptionRequest::new(
                MessageId::new(),
                session_id,
                [event.event_type().clone()],
            )),
        )
        .await;

        let mut saw_ack = false;
        let mut saw_event = false;
        let mut saw_query = false;
        while !saw_query {
            match next_outbound(&mut client).await {
                OutboundMessage::ActionAcknowledgement(_) => saw_ack = true,
                OutboundMessage::Event(_) => saw_event = true,
                OutboundMessage::QueryResponse(response) => {
                    assert_eq!(response.request_id(), &slow_id);
                    assert!(saw_ack, "Action Ack should not wait for slow Query");
                    assert!(saw_event, "Event should not wait for slow Query");
                    saw_query = true;
                }
                OutboundMessage::SubscriptionAccepted(_)
                | OutboundMessage::SubscriptionClosed(_) => {}
                other => panic!("unexpected response: {other:?}"),
            }
        }
        assert_eq!(action_calls.lock().await.len(), 1);
        drop(client);
        let _ = server_task.await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn oversized_query_response_is_mapped_to_safe_application_failure() {
        let query_executor = Arc::new(TestQueryExecutor {
            calls: Arc::new(AtomicUsize::new(0)),
            active: Arc::new(AtomicUsize::new(0)),
            max_active: Arc::new(AtomicUsize::new(0)),
        });
        let config = WebSocketAdapterConfig::new(
            TransportConfig::new(16, 512, 30_000, 10_000),
            16,
            16,
            90_000,
        );
        let (mut client, server_task) = connected_pair_with_query_config(
            ActorId::new(),
            query_executor,
            Arc::new(CompatibleVersionPolicy),
            config,
        )
        .await;
        send_inbound(&mut client, hello_query()).await;
        let _ = next_outbound(&mut client).await;
        send_inbound(&mut client, authenticate()).await;
        let request_id = MessageId::new();
        let mut payload = Payload::new();
        payload.insert("large", serde_json::json!(true));
        send_inbound(
            &mut client,
            query(QUERY_PROTOCOL_VERSION, request_id.clone(), payload),
        )
        .await;
        match next_outbound(&mut client).await {
            OutboundMessage::QueryResponse(response) => {
                assert_eq!(response.request_id(), &request_id);
                assert!(matches!(
                    response.result(),
                    QueryResult::Error(error)
                        if error.code() == orbitrelay_query::QueryFailureCode::Internal
                ));
            }
            other => panic!("expected safe query failure, got {other:?}"),
        }
        drop(client);
        let _ = server_task.await;
    }
}
