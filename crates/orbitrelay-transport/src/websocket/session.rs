//! Public WebSocket session entry points and dependency injection boundary.

use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::{tungstenite::protocol::Role, WebSocketStream};

use crate::{
    ActionExecutor, EventSourceFactory, IdentityResolver, JsonCodec, MessageCodec, QueryExecutor,
    SubscriptionAuthorizer, TransportConfig, TransportConnection, VersionPolicy,
};

use super::{coordinator::run_coordinator, error::WebSocketAdapterError};

const DEFAULT_WRITE_TIMEOUT_MILLISECONDS: u64 = 5_000;

/// Dependencies required by one WebSocket session.
///
/// The adapter only knows these transport ports. Runtime, EventBus, storage,
/// and authentication implementations stay outside this crate boundary.
#[derive(Clone)]
pub struct WebSocketSessionDependencies {
    pub(crate) action_executor: Arc<dyn ActionExecutor>,
    pub(crate) identity_resolver: Arc<dyn IdentityResolver>,
    pub(crate) subscription_authorizer: Arc<dyn SubscriptionAuthorizer>,
    pub(crate) event_source_factory: Arc<dyn EventSourceFactory>,
    pub(crate) version_policy: Arc<dyn VersionPolicy>,
    pub(crate) codec: Arc<dyn MessageCodec>,
    pub(crate) query_executor: Option<Arc<dyn QueryExecutor>>,
}

impl WebSocketSessionDependencies {
    /// Creates a dependency set for one WebSocket session.
    #[must_use]
    pub fn new(
        action_executor: Arc<dyn ActionExecutor>,
        identity_resolver: Arc<dyn IdentityResolver>,
        subscription_authorizer: Arc<dyn SubscriptionAuthorizer>,
        event_source_factory: Arc<dyn EventSourceFactory>,
        version_policy: Arc<dyn VersionPolicy>,
        codec: Arc<dyn MessageCodec>,
    ) -> Self {
        Self {
            action_executor,
            identity_resolver,
            subscription_authorizer,
            event_source_factory,
            version_policy,
            codec,
            query_executor: None,
        }
    }

    /// Creates dependencies using the built-in JSON codec.
    #[must_use]
    pub fn with_json_codec(
        action_executor: Arc<dyn ActionExecutor>,
        identity_resolver: Arc<dyn IdentityResolver>,
        subscription_authorizer: Arc<dyn SubscriptionAuthorizer>,
        event_source_factory: Arc<dyn EventSourceFactory>,
        version_policy: Arc<dyn VersionPolicy>,
    ) -> Self {
        Self::new(
            action_executor,
            identity_resolver,
            subscription_authorizer,
            event_source_factory,
            version_policy,
            Arc::new(JsonCodec),
        )
    }

    /// Adds the independent Query execution port used by protocol 0.2.
    #[must_use]
    pub fn with_query_executor(mut self, executor: Arc<dyn QueryExecutor>) -> Self {
        self.query_executor = Some(executor);
        self
    }
}

/// WebSocket-specific bounded task and heartbeat settings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebSocketAdapterConfig {
    transport: TransportConfig,
    inbound_capacity: usize,
    action_queue_capacity: usize,
    query_queue_capacity: usize,
    max_in_flight_queries: usize,
    heartbeat_timeout_milliseconds: u64,
    write_timeout_milliseconds: u64,
}

impl WebSocketAdapterConfig {
    /// Creates WebSocket settings from transport and adapter-specific limits.
    #[must_use]
    pub const fn new(
        transport: TransportConfig,
        inbound_capacity: usize,
        action_queue_capacity: usize,
        heartbeat_timeout_milliseconds: u64,
    ) -> Self {
        Self {
            transport,
            inbound_capacity,
            action_queue_capacity,
            query_queue_capacity: action_queue_capacity,
            max_in_flight_queries: 8,
            heartbeat_timeout_milliseconds,
            write_timeout_milliseconds: DEFAULT_WRITE_TIMEOUT_MILLISECONDS,
        }
    }

    /// Validates all transport and adapter limits.
    pub fn validate(&self) -> Result<(), WebSocketAdapterError> {
        self.transport.validate()?;
        if self.inbound_capacity == 0 {
            return Err(WebSocketAdapterError::Configuration(
                crate::TransportConfigError::NonZeroRequired {
                    field: "inbound_capacity",
                },
            ));
        }
        if self.action_queue_capacity == 0 {
            return Err(WebSocketAdapterError::Configuration(
                crate::TransportConfigError::NonZeroRequired {
                    field: "action_queue_capacity",
                },
            ));
        }
        if self.query_queue_capacity == 0 {
            return Err(WebSocketAdapterError::Configuration(
                crate::TransportConfigError::NonZeroRequired {
                    field: "query_queue_capacity",
                },
            ));
        }
        if self.max_in_flight_queries == 0 {
            return Err(WebSocketAdapterError::Configuration(
                crate::TransportConfigError::NonZeroRequired {
                    field: "max_in_flight_queries",
                },
            ));
        }
        if self.heartbeat_timeout_milliseconds == 0 {
            return Err(WebSocketAdapterError::Configuration(
                crate::TransportConfigError::NonZeroRequired {
                    field: "heartbeat_timeout_milliseconds",
                },
            ));
        }
        if self.write_timeout_milliseconds == 0 {
            return Err(WebSocketAdapterError::Configuration(
                crate::TransportConfigError::NonZeroRequired {
                    field: "write_timeout_milliseconds",
                },
            ));
        }
        Ok(())
    }

    /// Returns the shared transport limits.
    #[must_use]
    pub const fn transport(&self) -> &TransportConfig {
        &self.transport
    }

    /// Returns the inbound frame queue capacity.
    #[must_use]
    pub const fn inbound_capacity(&self) -> usize {
        self.inbound_capacity
    }

    /// Returns the sequential Action queue capacity.
    #[must_use]
    pub const fn action_queue_capacity(&self) -> usize {
        self.action_queue_capacity
    }

    /// Returns the bounded Query work queue capacity.
    #[must_use]
    pub const fn query_queue_capacity(&self) -> usize {
        self.query_queue_capacity
    }

    /// Returns the maximum number of concurrently executing Queries.
    #[must_use]
    pub const fn max_in_flight_queries(&self) -> usize {
        self.max_in_flight_queries
    }

    /// Sets the Query queue capacity.
    #[must_use]
    pub const fn with_query_queue_capacity(mut self, value: usize) -> Self {
        self.query_queue_capacity = value;
        self
    }

    /// Sets the maximum number of concurrently executing Queries.
    #[must_use]
    pub const fn with_max_in_flight_queries(mut self, value: usize) -> Self {
        self.max_in_flight_queries = value;
        self
    }

    /// Returns the heartbeat timeout in milliseconds.
    #[must_use]
    pub const fn heartbeat_timeout_milliseconds(&self) -> u64 {
        self.heartbeat_timeout_milliseconds
    }

    /// Returns the maximum duration of one WebSocket sink write.
    #[must_use]
    pub const fn write_timeout_milliseconds(&self) -> u64 {
        self.write_timeout_milliseconds
    }

    /// Sets the maximum duration of one WebSocket sink write.
    #[must_use]
    pub const fn with_write_timeout_milliseconds(mut self, value: u64) -> Self {
        self.write_timeout_milliseconds = value;
        self
    }
}

impl Default for WebSocketAdapterConfig {
    fn default() -> Self {
        Self::new(TransportConfig::default(), 64, 64, 90_000)
    }
}

/// Owns one established WebSocket stream and runs its complete adapter lifecycle.
pub struct WebSocketSession<S> {
    stream: WebSocketStream<S>,
    connection: TransportConnection,
    dependencies: WebSocketSessionDependencies,
    config: WebSocketAdapterConfig,
}

impl<S> WebSocketSession<S>
where
    S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    /// Creates a WebSocket session around an established stream.
    #[must_use]
    pub fn new(
        stream: WebSocketStream<S>,
        connection: TransportConnection,
        dependencies: WebSocketSessionDependencies,
        config: WebSocketAdapterConfig,
    ) -> Self {
        Self {
            stream,
            connection,
            dependencies,
            config,
        }
    }

    /// Runs the session until a normal or fatal close completes.
    pub async fn run(self) -> Result<(), WebSocketAdapterError> {
        run_coordinator(self.stream, self.connection, self.dependencies, self.config).await
    }
}

/// Runs one established WebSocket stream without binding a listener or router.
pub async fn run_websocket_session<S>(
    stream: WebSocketStream<S>,
    connection: TransportConnection,
    dependencies: WebSocketSessionDependencies,
    config: WebSocketAdapterConfig,
) -> Result<(), WebSocketAdapterError>
where
    S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    WebSocketSession::new(stream, connection, dependencies, config)
        .run()
        .await
}

/// Creates a server-side WebSocket stream over a raw established I/O stream.
///
/// This helper is intentionally limited to adapter tests and embedding code;
/// it does not bind a TCP port or perform an HTTP upgrade.
pub async fn from_raw_server_socket<S>(stream: S) -> WebSocketStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    WebSocketStream::from_raw_socket(stream, Role::Server, None).await
}

/// Creates a client-side WebSocket stream over a raw established I/O stream.
pub async fn from_raw_client_socket<S>(stream: S) -> WebSocketStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    WebSocketStream::from_raw_socket(stream, Role::Client, None).await
}

#[cfg(test)]
mod tests {
    use super::WebSocketAdapterConfig;
    use crate::WebSocketAdapterError;

    #[test]
    fn default_write_timeout_is_valid() {
        let config = WebSocketAdapterConfig::default();

        config.validate().expect("default config should be valid");
        assert_eq!(config.write_timeout_milliseconds(), 5_000);
    }

    #[test]
    fn zero_write_timeout_is_rejected() {
        let error = WebSocketAdapterConfig::default()
            .with_write_timeout_milliseconds(0)
            .validate()
            .expect_err("zero write timeout should be rejected");

        assert!(matches!(error, WebSocketAdapterError::Configuration(_)));
    }
}
