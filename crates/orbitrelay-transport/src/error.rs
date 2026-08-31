//! Internal transport errors and stable client-facing error messages.

use orbitrelay_core::Version;
use orbitrelay_protocol::{ActorId, MessageId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::ConnectionState;

/// Stable error codes exposed by the transport control protocol.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportErrorCode {
    /// The inbound message is malformed or structurally invalid.
    InvalidMessage,
    /// The client and server have no compatible protocol version.
    UnsupportedVersion,
    /// The requested operation is invalid for the connection state.
    ConnectionNotReady,
    /// The connection must authenticate before performing the operation.
    AuthenticationRequired,
    /// The action actor does not match the connection's trusted identity.
    IdentityMismatch,
    /// Action execution was rejected.
    ExecutionRejected,
    /// Subscription authorization was rejected.
    SubscriptionRejected,
    /// A subscription fell behind its event source.
    SubscriptionLagged,
    /// The connection cannot keep up with outbound messages.
    SlowConsumer,
    /// The selected codec could not encode a server message.
    CodecError,
    /// An internal transport dependency failed.
    InternalError,
}

/// A safe, stable error payload sent to a client.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorMessage {
    request_id: Option<MessageId>,
    code: TransportErrorCode,
    message: String,
    retryable: bool,
}

impl ErrorMessage {
    /// Builds a client-safe message from an internal transport error.
    #[must_use]
    pub fn from_transport_error(request_id: Option<MessageId>, error: &TransportError) -> Self {
        let code = error.code();
        Self {
            request_id,
            code,
            message: safe_message(code).to_owned(),
            retryable: error.is_retryable(),
        }
    }

    /// Returns the related request identifier, when one is available.
    #[must_use]
    pub const fn request_id(&self) -> Option<&MessageId> {
        self.request_id.as_ref()
    }

    /// Returns the stable transport error code.
    #[must_use]
    pub const fn code(&self) -> TransportErrorCode {
        self.code
    }

    /// Returns the client-safe description.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns whether retrying may succeed after addressing the condition.
    #[must_use]
    pub const fn retryable(&self) -> bool {
        self.retryable
    }
}

/// Failures while decoding or encoding transport messages.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum CodecError {
    /// The input is not valid JSON.
    #[error("invalid JSON input")]
    InvalidJson,
    /// The input does not have the required transport message shape.
    #[error("invalid transport message shape")]
    InvalidMessageShape,
    /// A required field is absent.
    #[error("transport message is missing field `{field}`")]
    MissingField {
        /// The missing field name.
        field: &'static str,
    },
    /// An unknown field was supplied.
    #[error("transport message contains unknown field `{field}`")]
    UnknownField {
        /// The unknown field name.
        field: String,
    },
    /// The inbound message kind is not supported.
    #[error("unsupported inbound message kind `{message_type}`")]
    UnsupportedMessageType {
        /// The unsupported message kind.
        message_type: String,
    },
    /// A protocol envelope carries an unexpected message type.
    #[error("expected `{expected}` envelope, received `{actual}`")]
    UnexpectedEnvelopeType {
        /// The expected envelope type.
        expected: &'static str,
        /// The received envelope type.
        actual: String,
    },
    /// Deserialization failed after structural validation.
    #[error("transport message could not be decoded")]
    DecodeFailed,
    /// Serialization of a server message failed.
    #[error("transport message could not be encoded")]
    EncodeFailed,
}

/// An invalid connection lifecycle operation.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum ConnectionStateError {
    /// The requested state transition is not allowed.
    #[error("connection cannot transition from {from:?} to {to:?}")]
    InvalidTransition {
        /// The current connection state.
        from: ConnectionState,
        /// The requested connection state.
        to: ConnectionState,
    },
    /// An operation is not valid in the current state.
    #[error("operation `{operation}` is not allowed in state {state:?}")]
    OperationNotAllowed {
        /// The attempted lifecycle operation.
        operation: &'static str,
        /// The current connection state.
        state: ConnectionState,
    },
}

/// Failures at the trusted connection identity boundary.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum IdentityError {
    /// The connection has not been bound to an actor.
    #[error("connection authentication is required")]
    AuthenticationRequired,
    /// The action actor differs from the trusted connection actor.
    #[error("action actor {action_actor_id} does not match bound actor {bound_actor_id}")]
    IdentityMismatch {
        /// The actor trusted for the connection.
        bound_actor_id: ActorId,
        /// The actor declared by the action.
        action_actor_id: ActorId,
    },
    /// Supplied credentials were rejected by the resolver.
    #[error("credentials were rejected: {detail}")]
    CredentialsRejected {
        /// Internal diagnostic detail that must not be sent to clients.
        detail: String,
    },
    /// The identity resolver is temporarily unavailable.
    #[error("identity resolver is unavailable: {detail}")]
    ResolverUnavailable {
        /// Internal diagnostic detail that must not be sent to clients.
        detail: String,
    },
    /// Identity resolution failed unexpectedly.
    #[error("identity resolution failed: {detail}")]
    ResolutionFailed {
        /// Internal diagnostic detail that must not be sent to clients.
        detail: String,
    },
}

/// Failures returned by an action execution adapter.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum TransportExecutionError {
    /// The action was rejected by the execution boundary.
    #[error("action execution was rejected: {detail}")]
    Rejected {
        /// Internal diagnostic detail that must not be sent to clients.
        detail: String,
    },
    /// The action executor is temporarily unavailable.
    #[error("action executor is unavailable: {detail}")]
    Unavailable {
        /// Internal diagnostic detail that must not be sent to clients.
        detail: String,
    },
    /// Action execution failed unexpectedly.
    #[error("action execution failed: {detail}")]
    Failed {
        /// Internal diagnostic detail that must not be sent to clients.
        detail: String,
    },
}

/// Failures returned by a transport event source.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum EventSourceError {
    /// The source dropped events because its consumer fell behind.
    #[error("event subscription lagged")]
    SubscriptionLagged,
    /// The event source is already closed.
    #[error("event subscription is closed")]
    SubscriptionClosed,
    /// The event source is temporarily unavailable.
    #[error("event source is unavailable: {detail}")]
    Unavailable {
        /// Internal diagnostic detail that must not be sent to clients.
        detail: String,
    },
    /// The event source failed unexpectedly.
    #[error("event source failed: {detail}")]
    Failed {
        /// Internal diagnostic detail that must not be sent to clients.
        detail: String,
    },
}

/// Failures returned by a subscription authorization adapter.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum SubscriptionAuthorizationError {
    /// The actor is not permitted to create the subscription.
    #[error("subscription was rejected: {detail}")]
    Rejected {
        /// Internal diagnostic detail that must not be sent to clients.
        detail: String,
    },
    /// The subscription authorizer is temporarily unavailable.
    #[error("subscription authorizer is unavailable: {detail}")]
    Unavailable {
        /// Internal diagnostic detail that must not be sent to clients.
        detail: String,
    },
    /// Subscription authorization failed unexpectedly.
    #[error("subscription authorization failed: {detail}")]
    Failed {
        /// Internal diagnostic detail that must not be sent to clients.
        detail: String,
    },
}

/// Failure to select a protocol version supported by both peers.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum VersionNegotiationError {
    /// No client-supported version satisfies the active policy.
    #[error("no compatible protocol version in {supported_versions:?}")]
    UnsupportedVersion {
        /// Versions advertised by the client.
        supported_versions: Vec<Version>,
    },
}

/// An invalid transport configuration value.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum TransportConfigError {
    /// A numeric configuration field must be greater than zero.
    #[error("transport configuration field `{field}` must be greater than zero")]
    NonZeroRequired {
        /// The invalid configuration field.
        field: &'static str,
    },
}

/// Internal errors crossing the transport orchestration boundary.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum TransportError {
    /// A message codec failed.
    #[error("transport codec failed: {0}")]
    Codec(#[from] CodecError),
    /// Protocol version negotiation failed.
    #[error("protocol version negotiation failed: {0}")]
    Version(#[from] VersionNegotiationError),
    /// A connection lifecycle operation failed.
    #[error("connection lifecycle operation failed: {0}")]
    ConnectionState(#[from] ConnectionStateError),
    /// Connection identity validation failed.
    #[error("connection identity validation failed: {0}")]
    Identity(#[from] IdentityError),
    /// The action execution port failed.
    #[error("action execution port failed: {0}")]
    Execution(#[from] TransportExecutionError),
    /// Subscription authorization failed.
    #[error("subscription authorization failed: {0}")]
    SubscriptionAuthorization(#[from] SubscriptionAuthorizationError),
    /// The event source failed.
    #[error("event source failed: {0}")]
    EventSource(#[from] EventSourceError),
    /// The outbound queue is full and the client cannot keep up.
    #[error("outbound queue reached its configured capacity")]
    SlowConsumer,
    /// An unexpected internal transport failure occurred.
    #[error("internal transport failure: {detail}")]
    Internal {
        /// Internal diagnostic detail that must not be sent to clients.
        detail: String,
    },
}

impl TransportError {
    /// Returns the stable external code corresponding to this internal error.
    #[must_use]
    pub const fn code(&self) -> TransportErrorCode {
        match self {
            Self::Codec(CodecError::EncodeFailed) => TransportErrorCode::CodecError,
            Self::Codec(_) => TransportErrorCode::InvalidMessage,
            Self::Version(_) => TransportErrorCode::UnsupportedVersion,
            Self::ConnectionState(_) => TransportErrorCode::ConnectionNotReady,
            Self::Identity(IdentityError::IdentityMismatch { .. }) => {
                TransportErrorCode::IdentityMismatch
            }
            Self::Identity(IdentityError::AuthenticationRequired)
            | Self::Identity(IdentityError::CredentialsRejected { .. })
            | Self::Identity(IdentityError::ResolverUnavailable { .. }) => {
                TransportErrorCode::AuthenticationRequired
            }
            Self::Identity(IdentityError::ResolutionFailed { .. }) => {
                TransportErrorCode::InternalError
            }
            Self::Execution(TransportExecutionError::Rejected { .. }) => {
                TransportErrorCode::ExecutionRejected
            }
            Self::Execution(_) => TransportErrorCode::InternalError,
            Self::SubscriptionAuthorization(SubscriptionAuthorizationError::Rejected {
                ..
            }) => TransportErrorCode::SubscriptionRejected,
            Self::SubscriptionAuthorization(_) => TransportErrorCode::InternalError,
            Self::EventSource(EventSourceError::SubscriptionLagged) => {
                TransportErrorCode::SubscriptionLagged
            }
            Self::EventSource(_) | Self::Internal { .. } => TransportErrorCode::InternalError,
            Self::SlowConsumer => TransportErrorCode::SlowConsumer,
        }
    }

    /// Returns whether the operation can reasonably be retried.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Identity(IdentityError::AuthenticationRequired)
                | Self::Identity(IdentityError::ResolverUnavailable { .. })
                | Self::Execution(TransportExecutionError::Unavailable { .. })
                | Self::SubscriptionAuthorization(
                    SubscriptionAuthorizationError::Unavailable { .. }
                )
                | Self::EventSource(EventSourceError::SubscriptionLagged)
                | Self::EventSource(EventSourceError::Unavailable { .. })
                | Self::SlowConsumer
        )
    }
}

const fn safe_message(code: TransportErrorCode) -> &'static str {
    match code {
        TransportErrorCode::InvalidMessage => "The message is invalid.",
        TransportErrorCode::UnsupportedVersion => "No compatible protocol version is available.",
        TransportErrorCode::ConnectionNotReady => "The connection is not ready for this operation.",
        TransportErrorCode::AuthenticationRequired => "Authentication is required.",
        TransportErrorCode::IdentityMismatch => {
            "The action identity does not match the connection identity."
        }
        TransportErrorCode::ExecutionRejected => "The action was rejected.",
        TransportErrorCode::SubscriptionRejected => "The subscription was rejected.",
        TransportErrorCode::SubscriptionLagged => "The subscription fell behind.",
        TransportErrorCode::SlowConsumer => "The connection cannot keep up with events.",
        TransportErrorCode::CodecError => "The server could not encode the message.",
        TransportErrorCode::InternalError => "An internal transport error occurred.",
    }
}

#[cfg(test)]
mod tests {
    use super::{ErrorMessage, TransportError, TransportErrorCode, TransportExecutionError};

    #[test]
    fn client_error_does_not_leak_internal_detail() {
        let secret = "database password=do-not-leak";
        let error = TransportError::Execution(TransportExecutionError::Failed {
            detail: secret.to_owned(),
        });
        let message = ErrorMessage::from_transport_error(None, &error);

        assert_eq!(message.code(), TransportErrorCode::InternalError);
        assert!(!message.message().contains(secret));
        assert_eq!(message.message(), "An internal transport error occurred.");
    }

    #[test]
    fn explicit_execution_rejection_has_stable_code() {
        let error = TransportError::Execution(TransportExecutionError::Rejected {
            detail: "policy detail".to_owned(),
        });
        let message = ErrorMessage::from_transport_error(None, &error);

        assert_eq!(message.code(), TransportErrorCode::ExecutionRejected);
        assert_eq!(message.message(), "The action was rejected.");
    }
}
