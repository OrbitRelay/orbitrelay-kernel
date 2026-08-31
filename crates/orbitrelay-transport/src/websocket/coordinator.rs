//! The single owner of one WebSocket connection's transport state.

use std::{collections::HashMap, sync::Arc, time::Duration};

use futures_util::StreamExt;
use orbitrelay_core::Version;
use orbitrelay_protocol::{ActionId, Event, MessageId};
use orbitrelay_query::{QueryActorContext, QueryFailure, QueryFailureCode, QueryResponse};
use tokio::{
    sync::mpsc,
    task::JoinHandle,
    time::{self, Instant},
};
use tokio_tungstenite::WebSocketStream;
use tokio_util::sync::CancellationToken;

use crate::{
    validate_action_binding, ActionAcknowledgement, ConnectionState, ErrorMessage, HelloAccepted,
    IdentityError, InboundMessage, JsonCodec, MessageCodec, OutboundMessage, SubscriptionAccepted,
    SubscriptionClosed, TransportConnection, TransportError, TransportExecutionError,
    TransportSubscriptionId,
};

use super::{
    action_worker::{run_action_worker, ActionJob},
    event_pump::run_event_pump,
    query_worker::{run_query_worker, QueryJob},
    reader::run_reader,
    session::{WebSocketAdapterConfig, WebSocketSessionDependencies},
    writer::run_writer,
    WebSocketAdapterError,
};

/// Classified input emitted by the reader task.
pub(crate) enum ReaderEvent {
    /// A WebSocket text frame, without decoding policy.
    Text(Vec<u8>),
    /// A WebSocket Ping control frame.
    NativePing(Vec<u8>),
    /// A WebSocket Pong control frame.
    NativePong,
    /// A peer close or clean EOF.
    Closed,
    /// A binary frame was received.
    Binary,
    /// A text or binary frame exceeded the configured limit.
    Oversized,
}

/// Commands accepted by the sink-owning writer task.
pub(crate) enum WriterCommand {
    /// Encodes and writes one application message.
    Application {
        /// Message to encode.
        message: OutboundMessage,
        /// Codec selected for this message.
        codec: Arc<dyn MessageCodec>,
    },
    /// Writes a native WebSocket Ping.
    NativePing(Vec<u8>),
    /// Writes a native WebSocket Pong.
    NativePong(Vec<u8>),
    /// Writes a native close and closes the sink.
    Close,
}

/// Results arriving asynchronously from worker and pump tasks.
pub(crate) enum CoordinatorEvent {
    /// A sequential Action worker completed.
    ActionCompleted {
        /// Original Action envelope request id.
        request_id: MessageId,
        /// Executed Action id.
        action_id: ActionId,
        /// Generated events or execution failure.
        result: Result<Vec<Event>, TransportExecutionError>,
    },
    /// A concurrent Query worker completed with a correlated response.
    QueryCompleted {
        /// Application response carrying the original request identity.
        response: QueryResponse,
    },
    /// A subscription event source completed normally.
    SubscriptionFinished {
        /// Completed subscription id.
        id: TransportSubscriptionId,
    },
    /// A subscription source failed.
    EventSourceFailed {
        /// Failed subscription id.
        id: TransportSubscriptionId,
        /// Transport-mapped source failure.
        error: TransportError,
        /// Whether the failure requires connection shutdown.
        fatal: bool,
        /// Whether the event pump queued the client-safe error already.
        error_queued: bool,
    },
    /// A bounded output queue rejected a pump event.
    SlowConsumer {
        /// Subscription whose event could not be queued.
        id: TransportSubscriptionId,
    },
    /// The writer task failed while touching the socket.
    WriterFailed {
        /// Internal writer diagnostic, including a distinct timeout variant.
        error: WebSocketAdapterError,
    },
    /// The reader task failed while touching the socket.
    ReaderFailed {
        /// Internal reader diagnostic.
        error: String,
    },
}

struct SubscriptionHandle {
    cancellation: CancellationToken,
    task: JoinHandle<()>,
}

/// Runs the coordinator and all child tasks for one established stream.
pub(crate) async fn run_coordinator<S>(
    stream: WebSocketStream<S>,
    mut connection: TransportConnection,
    dependencies: WebSocketSessionDependencies,
    config: WebSocketAdapterConfig,
) -> Result<(), WebSocketAdapterError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Unpin + 'static,
{
    config.validate()?;
    let (sink, input_stream) = stream.split();
    let (reader_sender, mut reader_receiver) = mpsc::channel(config.inbound_capacity());
    let (writer_sender, writer_receiver) = mpsc::channel(config.transport().outbound_capacity());
    let (event_sender, mut event_receiver) = mpsc::channel(config.inbound_capacity());
    let (action_sender, action_receiver) = mpsc::channel(config.action_queue_capacity());
    let (query_sender, query_receiver) = mpsc::channel(config.query_queue_capacity());
    let cancellation = CancellationToken::new();

    let reader_token = cancellation.child_token();
    let reader_event_sender = event_sender.clone();
    let reader_config = config.clone();
    let reader_task = tokio::spawn(async move {
        if let Err(error) = run_reader(
            input_stream,
            reader_sender,
            reader_token,
            reader_config.transport().max_message_bytes(),
        )
        .await
        {
            let _ = reader_event_sender
                .send(CoordinatorEvent::ReaderFailed { error })
                .await;
        }
    });

    let writer_token = cancellation.clone();
    let writer_event_sender = event_sender.clone();
    let write_timeout = Duration::from_millis(config.write_timeout_milliseconds());
    let mut writer_task = tokio::spawn(run_writer(
        sink,
        writer_receiver,
        writer_event_sender,
        writer_token,
        write_timeout,
    ));

    let action_token = cancellation.child_token();
    let action_event_sender = event_sender.clone();
    let action_task = tokio::spawn(run_action_worker(
        action_receiver,
        Arc::clone(&dependencies.action_executor),
        action_event_sender,
        action_token,
    ));

    let query_task = dependencies.query_executor.clone().map(|executor| {
        let query_token = cancellation.child_token();
        let query_event_sender = event_sender.clone();
        tokio::spawn(run_query_worker(
            query_receiver,
            executor,
            query_event_sender,
            query_token,
            config.max_in_flight_queries(),
        ))
    });

    let bootstrap_codec: Arc<dyn MessageCodec> = Arc::new(JsonCodec);
    let mut active_codec = Arc::clone(&bootstrap_codec);
    let mut negotiated_version: Option<Version> = None;
    let mut subscriptions: HashMap<TransportSubscriptionId, SubscriptionHandle> = HashMap::new();
    let mut last_activity = Instant::now();
    let negotiation_started = Instant::now();
    let mut heartbeat = time::interval(Duration::from_millis(
        config.transport().heartbeat_interval_milliseconds(),
    ));
    heartbeat.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
    let mut fatal_error: Option<WebSocketAdapterError> = None;
    let mut fatal_error_queued = false;
    let mut graceful_close = false;

    'coordinator: loop {
        tokio::select! {
            input = reader_receiver.recv() => {
                let Some(input) = input else {
                    graceful_close = true;
                    break 'coordinator;
                };
                last_activity = Instant::now();
                match input {
                    ReaderEvent::Text(bytes) => {
                        let decoded = active_codec.decode_inbound(&bytes);
                        let message = match decoded {
                            Ok(message) => message,
                            Err(error) => {
                                let transport_error = TransportError::Codec(error);
                                if !queue_error(
                                    &writer_sender,
                                    Arc::clone(&active_codec),
                                    None,
                                    &transport_error,
                                ) {
                                    fatal_error = Some(WebSocketAdapterError::Transport(
                                        TransportError::SlowConsumer,
                                    ));
                                    break 'coordinator;
                                }
                                continue;
                            }
                        };
                        if matches!(&message, InboundMessage::Close(_)) {
                            graceful_close = true;
                            break 'coordinator;
                        }
                        if let Err(error) = handle_inbound(
                            message,
                            &mut connection,
                            &dependencies,
                            &writer_sender,
                            &action_sender,
                            &query_sender,
                            &event_sender,
                            &mut active_codec,
                            &bootstrap_codec,
                            &mut negotiated_version,
                            &mut subscriptions,
                            &cancellation,
                            config.transport().max_message_bytes(),
                        ).await {
                            let fatal = matches!(
                                error,
                                WebSocketAdapterError::ConnectionState(
                                    crate::ConnectionStateError::InvalidTransition { .. },
                                )
                                    | WebSocketAdapterError::Transport(TransportError::Version(_))
                                    | WebSocketAdapterError::Frame(_)
                            );
                            let transport_error = match &error {
                                WebSocketAdapterError::Transport(error) => error.clone(),
                                WebSocketAdapterError::ConnectionState(error) => {
                                    TransportError::ConnectionState(error.clone())
                                }
                                _ => TransportError::Internal {
                                    detail: error.to_string(),
                                },
                            };
                            let queued = queue_error(
                                &writer_sender,
                                Arc::clone(&active_codec),
                                None,
                                &transport_error,
                            );
                            if fatal || !queued {
                                fatal_error_queued = queued;
                                fatal_error = Some(error);
                                break 'coordinator;
                            }
                        }
                    }
                    ReaderEvent::NativePing(payload) => {
                        if writer_sender
                            .try_send(WriterCommand::NativePong(payload))
                            .is_err()
                        {
                            fatal_error = Some(WebSocketAdapterError::Transport(
                                TransportError::SlowConsumer,
                            ));
                            break 'coordinator;
                        }
                    }
                    ReaderEvent::NativePong => {}
                    ReaderEvent::Closed => {
                        graceful_close = true;
                        break 'coordinator;
                    }
                    ReaderEvent::Binary => {
                        let error = TransportError::Codec(crate::CodecError::InvalidMessageShape);
                        fatal_error_queued = queue_error(
                            &writer_sender,
                            Arc::clone(&active_codec),
                            None,
                            &error,
                        );
                        fatal_error = Some(WebSocketAdapterError::Frame(
                            "binary WebSocket frames are not supported".to_owned(),
                        ));
                        break 'coordinator;
                    }
                    ReaderEvent::Oversized => {
                        let error = TransportError::Codec(crate::CodecError::InvalidMessageShape);
                        fatal_error_queued = queue_error(
                            &writer_sender,
                            Arc::clone(&active_codec),
                            None,
                            &error,
                        );
                        fatal_error = Some(WebSocketAdapterError::Frame(
                            "WebSocket message exceeds max_message_bytes".to_owned(),
                        ));
                        break 'coordinator;
                    }
                }
            }
            event = event_receiver.recv() => {
                let Some(event) = event else {
                    fatal_error = Some(WebSocketAdapterError::Task(
                        "coordinator event channel closed".to_owned(),
                    ));
                    break 'coordinator;
                };
                match event {
                    CoordinatorEvent::ActionCompleted { request_id, action_id, result } => {
                        match result {
                            Ok(events) => {
                                let acknowledgement = OutboundMessage::ActionAcknowledgement(
                                    ActionAcknowledgement::new(
                                        request_id,
                                        action_id,
                                        events.iter().map(|event| event.id().clone()).collect(),
                                    ),
                                );
                                if !queue_application(
                                    &writer_sender,
                                    Arc::clone(&active_codec),
                                    acknowledgement,
                                ) {
                                    fatal_error = Some(WebSocketAdapterError::Transport(
                                        TransportError::SlowConsumer,
                                    ));
                                    break 'coordinator;
                                }
                            }
                            Err(error) => {
                                let error = TransportError::Execution(error);
                                if !queue_error(
                                    &writer_sender,
                                    Arc::clone(&active_codec),
                                    Some(request_id),
                                    &error,
                                ) {
                                    fatal_error = Some(WebSocketAdapterError::Transport(
                                        TransportError::SlowConsumer,
                                    ));
                                    break 'coordinator;
                                }
                            }
                        }
                    }
                    CoordinatorEvent::QueryCompleted { response } => {
                        let version = negotiated_version.unwrap_or(crate::QUERY_PROTOCOL_VERSION);
                        if !queue_query_response(
                            &writer_sender,
                            Arc::clone(&active_codec),
                            version,
                            response,
                            config.transport().max_message_bytes(),
                        ) {
                            fatal_error = Some(WebSocketAdapterError::Transport(
                                TransportError::SlowConsumer,
                            ));
                            break 'coordinator;
                        }
                    }
                    CoordinatorEvent::SubscriptionFinished { id } => {
                        if let Some(handle) = subscriptions.remove(&id) {
                            handle.cancellation.cancel();
                            let _ = handle.task.await;
                            let closed = OutboundMessage::SubscriptionClosed(
                                SubscriptionClosed::new(None, id),
                            );
                            if !queue_application(
                                &writer_sender,
                                Arc::clone(&active_codec),
                                closed,
                            ) {
                                fatal_error = Some(WebSocketAdapterError::Transport(
                                    TransportError::SlowConsumer,
                                ));
                                break 'coordinator;
                            }
                        }
                    }
                    CoordinatorEvent::EventSourceFailed {
                        id,
                        error,
                        fatal,
                        error_queued,
                    } => {
                        if fatal {
                            fatal_error_queued = error_queued;
                            fatal_error = Some(WebSocketAdapterError::Transport(error));
                            break 'coordinator;
                        }
                        if let Some(handle) = subscriptions.remove(&id) {
                            handle.cancellation.cancel();
                            let _ = handle.task.await;
                        }
                    }
                    CoordinatorEvent::SlowConsumer { id } => {
                        let _ = id;
                        let error = TransportError::SlowConsumer;
                        fatal_error_queued = queue_error(
                            &writer_sender,
                            Arc::clone(&active_codec),
                            None,
                            &error,
                        );
                        fatal_error = Some(WebSocketAdapterError::Transport(error));
                        break 'coordinator;
                    }
                    CoordinatorEvent::WriterFailed { error } => {
                        fatal_error = Some(error);
                        break 'coordinator;
                    }
                    CoordinatorEvent::ReaderFailed { error } => {
                        fatal_error = Some(WebSocketAdapterError::Task(error));
                        break 'coordinator;
                    }
                }
            }
            _ = heartbeat.tick() => {
                if connection.state() == ConnectionState::Negotiating
                    && negotiation_started.elapsed()
                        > Duration::from_millis(
                            config.transport().negotiation_timeout_milliseconds(),
                        )
                {
                    fatal_error = Some(WebSocketAdapterError::Frame(
                        "protocol negotiation timed out".to_owned(),
                    ));
                    break 'coordinator;
                }
                if last_activity.elapsed()
                    > Duration::from_millis(config.heartbeat_timeout_milliseconds())
                {
                    fatal_error = Some(WebSocketAdapterError::Frame(
                        "WebSocket heartbeat timed out".to_owned(),
                    ));
                    break 'coordinator;
                }
                if writer_sender
                    .try_send(WriterCommand::NativePing(Vec::new()))
                    .is_err()
                {
                    fatal_error = Some(WebSocketAdapterError::Transport(
                        TransportError::SlowConsumer,
                    ));
                    break 'coordinator;
                }
            }
        }
    }

    if !graceful_close {
        if let Some(error) = &fatal_error {
            if fatal_error_queued {
                // The fatal branch already queued its stable client error.
            } else {
                let transport_error = match error {
                    WebSocketAdapterError::Transport(error) => error.clone(),
                    _ => TransportError::Internal {
                        detail: error.to_string(),
                    },
                };
                let _ = queue_error(
                    &writer_sender,
                    Arc::clone(&active_codec),
                    None,
                    &transport_error,
                );
            }
        }
    }
    let writer_close_queued = writer_sender.send(WriterCommand::Close).await.is_ok();
    if connection.state() != ConnectionState::Closed {
        if connection.state() != ConnectionState::Closing {
            let _ = connection.begin_close();
        }
        if connection.state() == ConnectionState::Closing {
            let _ = connection.close();
        }
    }
    let mut writer_finished = false;
    if writer_close_queued {
        writer_finished = time::timeout(Duration::from_secs(1), &mut writer_task)
            .await
            .is_ok();
    }
    cancellation.cancel();
    drop(action_sender);
    drop(query_sender);
    for (_, handle) in subscriptions.drain() {
        handle.cancellation.cancel();
        let _ = handle.task.await;
    }
    let _ = reader_task.await;
    let _ = action_task.await;
    if let Some(query_task) = query_task {
        let _ = query_task.await;
    }
    if !writer_finished {
        let _ = writer_task.await;
    }

    fatal_error.map_or(Ok(()), Err)
}

#[allow(
    clippy::too_many_arguments,
    reason = "the coordinator passes explicit ownership boundaries to avoid shared mutable state"
)]
async fn handle_inbound(
    message: InboundMessage,
    connection: &mut TransportConnection,
    dependencies: &WebSocketSessionDependencies,
    writer: &mpsc::Sender<WriterCommand>,
    actions: &mpsc::Sender<ActionJob>,
    queries: &mpsc::Sender<QueryJob>,
    coordinator: &mpsc::Sender<CoordinatorEvent>,
    active_codec: &mut Arc<dyn MessageCodec>,
    bootstrap_codec: &Arc<dyn MessageCodec>,
    negotiated_version: &mut Option<Version>,
    subscriptions: &mut HashMap<TransportSubscriptionId, SubscriptionHandle>,
    cancellation: &CancellationToken,
    config_message_limit: usize,
) -> Result<(), WebSocketAdapterError> {
    match message {
        InboundMessage::Hello(hello) => {
            if connection.state() != ConnectionState::Negotiating {
                return Err(WebSocketAdapterError::Transport(TransportError::Internal {
                    detail: "duplicate or late hello".to_owned(),
                }));
            }
            if !hello.codecs().iter().any(|codec| codec == "json") || active_codec.name() != "json"
            {
                return Err(WebSocketAdapterError::Transport(TransportError::Codec(
                    crate::CodecError::UnsupportedMessageType {
                        message_type: "codec".to_owned(),
                    },
                )));
            }
            let version = dependencies
                .version_policy
                .negotiate(hello.supported_versions())
                .map_err(|error| {
                    WebSocketAdapterError::Transport(TransportError::Version(error))
                })?;
            connection.begin_authentication()?;
            *negotiated_version = Some(version);
            let accepted = OutboundMessage::HelloAccepted(HelloAccepted::new(version, "json"));
            if !queue_application(writer, Arc::clone(bootstrap_codec), accepted) {
                return Err(WebSocketAdapterError::Transport(
                    TransportError::SlowConsumer,
                ));
            }
            *active_codec = Arc::clone(&dependencies.codec);
            Ok(())
        }
        InboundMessage::Authenticate(authenticate) => {
            if connection.state() != ConnectionState::Authenticating {
                return Err(WebSocketAdapterError::Transport(
                    TransportError::ConnectionState(
                        crate::ConnectionStateError::OperationNotAllowed {
                            operation: "authenticate",
                            state: connection.state(),
                        },
                    ),
                ));
            }
            match dependencies
                .identity_resolver
                .resolve(connection.id(), authenticate.credentials())
                .await
            {
                Ok(binding) => {
                    connection.bind_identity(binding)?;
                    connection.mark_ready()?;
                    Ok(())
                }
                Err(error) => Err(WebSocketAdapterError::Transport(TransportError::Identity(
                    error,
                ))),
            }
        }
        InboundMessage::Ping(ping) => {
            if !queue_application(
                writer,
                Arc::clone(active_codec),
                OutboundMessage::Pong(crate::PongMessage::new(ping.nonce())),
            ) {
                return Err(WebSocketAdapterError::Transport(
                    TransportError::SlowConsumer,
                ));
            }
            Ok(())
        }
        InboundMessage::Close(_) => Ok(()),
        InboundMessage::Action(envelope) => {
            ensure_ready(connection)?;
            validate_action_binding(connection.identity(), envelope.payload()).map_err(
                |error| WebSocketAdapterError::Transport(TransportError::Identity(error)),
            )?;
            actions
                .try_send(ActionJob {
                    request_id: envelope.message_id().clone(),
                    action: envelope.into_payload(),
                })
                .map_err(|_| {
                    WebSocketAdapterError::Transport(TransportError::Execution(
                        TransportExecutionError::Unavailable {
                            detail: "action queue is full".to_owned(),
                        },
                    ))
                })?;
            Ok(())
        }
        InboundMessage::Query(message) => {
            ensure_ready(connection)?;
            let Some(negotiated) = *negotiated_version else {
                return Err(WebSocketAdapterError::Transport(TransportError::Version(
                    crate::VersionNegotiationError::UnsupportedVersion {
                        supported_versions: vec![crate::QUERY_PROTOCOL_VERSION],
                    },
                )));
            };
            if negotiated != crate::QUERY_PROTOCOL_VERSION
                || message.version() != crate::QUERY_PROTOCOL_VERSION
            {
                let error =
                    TransportError::Version(crate::VersionNegotiationError::UnsupportedVersion {
                        supported_versions: vec![crate::QUERY_PROTOCOL_VERSION],
                    });
                if !queue_error(
                    writer,
                    Arc::clone(active_codec),
                    Some(message.message_id().clone()),
                    &error,
                ) {
                    return Err(WebSocketAdapterError::Transport(
                        TransportError::SlowConsumer,
                    ));
                }
                return Ok(());
            }
            let binding = connection
                .identity()
                .ok_or(WebSocketAdapterError::Transport(TransportError::Identity(
                    IdentityError::AuthenticationRequired,
                )))?;
            let request_id = message.message_id().clone();
            let query_type = message.message_type().clone();
            let request = message.into_request();
            let actor = QueryActorContext::new(binding.actor_id().clone());
            if dependencies.query_executor.is_none() {
                let response = QueryResponse::error(
                    request_id,
                    query_type,
                    QueryFailure::new(
                        QueryFailureCode::Unavailable,
                        "The query service is temporarily unavailable.",
                        true,
                    ),
                );
                if !queue_query_response(
                    writer,
                    Arc::clone(active_codec),
                    negotiated,
                    response,
                    config_message_limit,
                ) {
                    return Err(WebSocketAdapterError::Transport(
                        TransportError::SlowConsumer,
                    ));
                }
                return Ok(());
            }
            if queries.try_send(QueryJob { actor, request }).is_err() {
                // A decoded Query remains an application request even when
                // admission is saturated. Return a correlated QueryResponse
                // instead of disguising backpressure as a wire/transport
                // failure.
                let response = QueryResponse::error(
                    request_id,
                    query_type,
                    QueryFailure::new(
                        QueryFailureCode::Unavailable,
                        "The query service is temporarily unavailable.",
                        true,
                    ),
                );
                if !queue_query_response(
                    writer,
                    Arc::clone(active_codec),
                    negotiated,
                    response,
                    config_message_limit,
                ) {
                    return Err(WebSocketAdapterError::Transport(
                        TransportError::SlowConsumer,
                    ));
                }
            }
            Ok(())
        }
        InboundMessage::Subscribe(request) => {
            ensure_ready(connection)?;
            let binding = connection
                .identity()
                .ok_or(WebSocketAdapterError::Transport(TransportError::Identity(
                    IdentityError::AuthenticationRequired,
                )))?;
            dependencies
                .subscription_authorizer
                .authorize(binding, &request)
                .await
                .map_err(|error| {
                    WebSocketAdapterError::Transport(TransportError::SubscriptionAuthorization(
                        error,
                    ))
                })?;
            let source = dependencies
                .event_source_factory
                .subscribe(request.clone())
                .await
                .map_err(|error| {
                    WebSocketAdapterError::Transport(TransportError::EventSource(error))
                })?;
            let id = source.id().clone();
            if subscriptions.contains_key(&id) {
                return Err(WebSocketAdapterError::Transport(TransportError::Internal {
                    detail: "duplicate subscription identifier".to_owned(),
                }));
            }
            let version = negotiated_version.ok_or(TransportError::Version(
                crate::VersionNegotiationError::UnsupportedVersion {
                    supported_versions: Vec::new(),
                },
            ))?;
            if !queue_application(
                writer,
                Arc::clone(active_codec),
                OutboundMessage::SubscriptionAccepted(SubscriptionAccepted::new(
                    request.request_id().clone(),
                    id.clone(),
                )),
            ) {
                let mut source = source;
                let _ = source.close().await;
                return Err(WebSocketAdapterError::Transport(
                    TransportError::SlowConsumer,
                ));
            }
            let subscription_cancellation = cancellation.child_token();
            let task = tokio::spawn(run_event_pump(
                id.clone(),
                source,
                version,
                Arc::clone(active_codec),
                writer.clone(),
                coordinator.clone(),
                subscription_cancellation.clone(),
            ));
            subscriptions.insert(
                id.clone(),
                SubscriptionHandle {
                    cancellation: subscription_cancellation,
                    task,
                },
            );
            Ok(())
        }
        InboundMessage::Unsubscribe(unsubscribe) => {
            ensure_ready(connection)?;
            let id = unsubscribe.subscription_id().clone();
            let Some(handle) = subscriptions.remove(&id) else {
                let error = TransportError::SubscriptionAuthorization(
                    crate::SubscriptionAuthorizationError::Rejected {
                        detail: "subscription does not exist".to_owned(),
                    },
                );
                if !queue_error(
                    writer,
                    Arc::clone(active_codec),
                    Some(unsubscribe.request_id().clone()),
                    &error,
                ) {
                    return Err(WebSocketAdapterError::Transport(
                        TransportError::SlowConsumer,
                    ));
                }
                return Ok(());
            };
            handle.cancellation.cancel();
            let _ = handle.task.await;
            if !queue_application(
                writer,
                Arc::clone(active_codec),
                OutboundMessage::SubscriptionClosed(SubscriptionClosed::new(
                    Some(unsubscribe.request_id().clone()),
                    id,
                )),
            ) {
                return Err(WebSocketAdapterError::Transport(
                    TransportError::SlowConsumer,
                ));
            }
            Ok(())
        }
    }
}

fn ensure_ready(connection: &TransportConnection) -> Result<(), WebSocketAdapterError> {
    match connection.state() {
        ConnectionState::Ready => Ok(()),
        ConnectionState::Authenticating => Err(WebSocketAdapterError::Transport(
            TransportError::Identity(IdentityError::AuthenticationRequired),
        )),
        state => Err(WebSocketAdapterError::Transport(
            TransportError::ConnectionState(crate::ConnectionStateError::OperationNotAllowed {
                operation: "data_plane_message",
                state,
            }),
        )),
    }
}

fn queue_application(
    writer: &mpsc::Sender<WriterCommand>,
    codec: Arc<dyn MessageCodec>,
    message: OutboundMessage,
) -> bool {
    writer
        .try_send(WriterCommand::Application { message, codec })
        .is_ok()
}

fn queue_query_response(
    writer: &mpsc::Sender<WriterCommand>,
    codec: Arc<dyn MessageCodec>,
    version: Version,
    response: QueryResponse,
    max_message_bytes: usize,
) -> bool {
    let request_id = response.request_id().clone();
    let query_type = response.query_type().clone();
    let message = OutboundMessage::QueryResponse(crate::QueryResponseMessage::from_response(
        version, response,
    ));

    match codec.encode_outbound(&message) {
        Ok(encoded) if encoded.len() <= max_message_bytes => {
            queue_application(writer, codec, message)
        }
        Ok(_) => {
            let fallback = QueryResponse::error(
                request_id,
                query_type,
                QueryFailure::new(
                    QueryFailureCode::Internal,
                    "The query response exceeds the configured message limit.",
                    false,
                ),
            );
            let fallback_message = OutboundMessage::QueryResponse(
                crate::QueryResponseMessage::from_response(version, fallback),
            );
            match codec.encode_outbound(&fallback_message) {
                Ok(encoded) if encoded.len() <= max_message_bytes => {
                    queue_application(writer, codec, fallback_message)
                }
                _ => false,
            }
        }
        Err(_) => false,
    }
}

fn queue_error(
    writer: &mpsc::Sender<WriterCommand>,
    codec: Arc<dyn MessageCodec>,
    request_id: Option<MessageId>,
    error: &TransportError,
) -> bool {
    queue_application(
        writer,
        codec,
        OutboundMessage::Error(ErrorMessage::from_transport_error(request_id, error)),
    )
}
