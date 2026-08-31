//! WebSocket adapter diagnostics that do not expose tungstenite internals.

use crate::{CodecError, ConnectionStateError, TransportConfigError, TransportError};
use thiserror::Error;

/// Errors returned by a WebSocket session after its close procedure completes.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum WebSocketAdapterError {
    /// The adapter configuration is invalid.
    #[error("invalid WebSocket adapter configuration: {0}")]
    Configuration(#[from] TransportConfigError),
    /// Transport state or dependency processing failed.
    #[error("WebSocket transport failed: {0}")]
    Transport(#[from] TransportError),
    /// The connection entered an illegal lifecycle state.
    #[error("WebSocket connection state failed: {0}")]
    ConnectionState(#[from] ConnectionStateError),
    /// A WebSocket frame was invalid or unsupported.
    #[error("WebSocket frame was invalid: {0}")]
    Frame(String),
    /// Encoding an outbound application message failed.
    #[error("WebSocket outbound encoding failed: {0}")]
    Codec(#[from] CodecError),
    /// A WebSocket sink operation did not complete before its configured deadline.
    #[error("WebSocket {operation} timed out after {timeout_milliseconds} milliseconds")]
    WriteTimeout {
        /// The bounded sink operation.
        operation: &'static str,
        /// The configured timeout in milliseconds.
        timeout_milliseconds: u64,
    },
    /// A required internal task terminated unexpectedly.
    #[error("WebSocket task failed: {0}")]
    Task(String),
}
