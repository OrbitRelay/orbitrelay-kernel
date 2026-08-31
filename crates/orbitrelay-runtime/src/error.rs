//! Errors produced by the protocol runtime and its extension ports.

use orbitrelay_protocol::{ActionId, ActionType};
use thiserror::Error;

use crate::ExecutionScope;

/// An error returned while validating or handling an action.
#[derive(Debug, Error)]
#[error("{message}")]
pub struct HandlerError {
    message: String,
}

impl HandlerError {
    /// Creates a handler error with a stable, human-readable message.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Returns the handler error message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// An error returned by an external action authorizer.
#[derive(Debug, Error)]
#[error("{message}")]
pub struct AuthorizationError {
    message: String,
}

impl AuthorizationError {
    /// Creates an authorization error.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Returns the authorization error message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// An error returned while dispatching generated events.
#[derive(Debug, Error)]
#[error("{message}")]
pub struct PipelineError {
    message: String,
}

/// An error produced while creating a scope or acquiring its execution lease.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ExecutionCoordinationError {
    /// A scope field was empty or contained only whitespace.
    #[error("execution scope field `{field}` must not be empty")]
    InvalidScope {
        /// The invalid scope field.
        field: &'static str,
    },

    /// The configured coordination service is temporarily unavailable.
    #[error("execution coordinator is unavailable: {message}")]
    Unavailable {
        /// A diagnostic message for operators.
        message: String,
    },

    /// The coordinator could not acquire the requested lease.
    #[error("execution lease acquisition failed: {message}")]
    AcquisitionFailed {
        /// A diagnostic message for operators.
        message: String,
    },
}

impl ExecutionCoordinationError {
    /// Creates an unavailable-coordinator error.
    #[must_use]
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::Unavailable {
            message: message.into(),
        }
    }

    /// Creates a lease-acquisition error.
    #[must_use]
    pub fn acquisition_failed(message: impl Into<String>) -> Self {
        Self::AcquisitionFailed {
            message: message.into(),
        }
    }
}

impl PipelineError {
    /// Creates an event pipeline error.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Returns the pipeline error message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Errors produced while changing the handler registry.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RegistryError {
    /// An action type already has a registered handler.
    #[error("handler already registered for action type `{action_type}`")]
    AlreadyRegistered {
        /// The action type whose registration would be replaced.
        action_type: ActionType,
    },
}

/// Errors produced while executing an action lifecycle.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RuntimeError {
    /// No handler is registered for the requested action type.
    #[error("handler not found for action type `{action_type}`")]
    HandlerNotFound {
        /// The unhandled action type.
        action_type: ActionType,
    },

    /// The selected handler rejected the action during validation.
    #[error("validation failed for action `{action_id}`: {source}")]
    ValidationFailed {
        /// The action that failed validation.
        action_id: ActionId,
        /// The handler validation error.
        #[source]
        source: HandlerError,
    },

    /// The external authorizer rejected the action.
    #[error("authorization failed for action `{action_id}`: {source}")]
    AuthorizationFailed {
        /// The action that was rejected.
        action_id: ActionId,
        /// The authorization error.
        #[source]
        source: AuthorizationError,
    },

    /// A scoped handler ran without a configured execution coordinator.
    #[error("execution coordination is unavailable for action `{action_id}` and scope {scope:?}")]
    CoordinationUnavailable {
        /// The action that required coordination.
        action_id: ActionId,
        /// The scope that could not be coordinated.
        scope: ExecutionScope,
    },

    /// The configured coordinator failed to acquire an execution lease.
    #[error(
        "execution coordination failed for action `{action_id}` and scope {scope:?}: {source}"
    )]
    CoordinationFailed {
        /// The action whose lease could not be acquired.
        action_id: ActionId,
        /// The scope whose acquisition failed.
        scope: ExecutionScope,
        /// The internal coordination failure.
        #[source]
        source: ExecutionCoordinationError,
    },

    /// The selected handler failed while producing event drafts.
    #[error("handler failed for action `{action_id}`: {source}")]
    HandlerFailed {
        /// The action whose handler failed.
        action_id: ActionId,
        /// The handler execution error.
        #[source]
        source: HandlerError,
    },

    /// Generated events could not be dispatched through the pipeline.
    #[error("pipeline failed for action `{action_id}`: {source}")]
    PipelineFailed {
        /// The action whose events could not be dispatched.
        action_id: ActionId,
        /// The pipeline error.
        #[source]
        source: PipelineError,
    },
}

#[cfg(test)]
mod tests {
    use super::{AuthorizationError, HandlerError, PipelineError};

    #[test]
    fn preserves_boundary_error_messages() {
        assert_eq!(
            HandlerError::new("invalid payload").message(),
            "invalid payload"
        );
        assert_eq!(AuthorizationError::new("denied").message(), "denied");
        assert_eq!(PipelineError::new("unavailable").message(), "unavailable");
    }
}
