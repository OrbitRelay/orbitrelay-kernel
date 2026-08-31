//! Transport-independent connection and message boundaries for OrbitRelay.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod codec;
mod config;
mod connection;
mod error;
mod executor;
mod identity;
mod message;
mod source;
mod subscription;
mod version;
mod websocket;

pub use codec::{JsonCodec, MessageCodec};
pub use config::TransportConfig;
pub use connection::{ConnectionId, ConnectionMetadata, ConnectionState, TransportConnection};
pub use error::{
    CodecError, ConnectionStateError, ErrorMessage, EventSourceError, IdentityError,
    SubscriptionAuthorizationError, TransportConfigError, TransportError, TransportErrorCode,
    TransportExecutionError, VersionNegotiationError,
};
pub use executor::ActionExecutor;
pub use identity::{
    validate_action_binding, ActorBinding, IdentityResolver, IdentitySource, InboundCredentials,
};
pub use message::{
    ActionAcknowledgement, Authenticate, CloseMessage, Hello, HelloAccepted, InboundMessage,
    OutboundMessage, PingMessage, PongMessage, QueryMessage, QueryResponseMessage,
    SubscriptionAccepted, SubscriptionClosed, Unsubscribe,
};
pub use orbitrelay_query::QueryExecutor;
pub use source::{EventSource, EventSourceFactory};
pub use subscription::{SubscriptionAuthorizer, SubscriptionRequest, TransportSubscriptionId};
pub use version::{
    CompatibleVersionPolicy, ExactQueryVersionPolicy, ExactVersionPolicy, VersionPolicy,
    CURRENT_PROTOCOL_VERSION, QUERY_PROTOCOL_VERSION,
};
pub use websocket::{
    from_raw_client_socket, from_raw_server_socket, run_websocket_session, WebSocketAdapterConfig,
    WebSocketAdapterError, WebSocketSession, WebSocketSessionDependencies,
};
