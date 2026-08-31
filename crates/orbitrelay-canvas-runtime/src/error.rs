//! Canvas runtime port and command-processing errors.

use orbitrelay_canvas::{CanvasError, CanvasId, CanvasProjectionError, LayerId, StrokeId};
use orbitrelay_protocol::SessionId;
use orbitrelay_runtime::HandlerError;
use thiserror::Error;

/// Failure reported by a Canvas metadata catalog implementation.
#[derive(Debug, Error)]
#[error("Canvas catalog query failed")]
pub struct CanvasCatalogError {
    detail: String,
}

impl CanvasCatalogError {
    /// Creates a catalog error with operator-facing diagnostic detail.
    #[must_use]
    pub fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }

    /// Returns operator-facing diagnostic detail.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

/// Failure reported while reading an already-persisted Stroke projection.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CanvasStateReadError {
    /// The state source could not complete the read.
    #[error("Canvas state source is unavailable")]
    Unavailable {
        /// Operator-facing diagnostic detail.
        detail: String,
    },
    /// Persisted Canvas history could not be projected safely.
    #[error("persisted Canvas history is corrupted")]
    ProjectionCorrupted {
        /// The deterministic projection failure.
        #[source]
        source: CanvasProjectionError,
    },
}

impl CanvasStateReadError {
    /// Creates an unavailable-state-source error.
    #[must_use]
    pub fn unavailable(detail: impl Into<String>) -> Self {
        Self::Unavailable {
            detail: detail.into(),
        }
    }

    /// Creates a corrupted-projection error.
    #[must_use]
    pub const fn projection_corrupted(source: CanvasProjectionError) -> Self {
        Self::ProjectionCorrupted { source }
    }

    /// Returns optional operator-facing source detail.
    #[must_use]
    pub fn detail(&self) -> Option<&str> {
        match self {
            Self::Unavailable { detail } => Some(detail),
            Self::ProjectionCorrupted { .. } => None,
        }
    }
}

/// Errors produced by Canvas command handling after authorization.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CanvasRuntimeError {
    /// The requested Canvas does not exist.
    #[error("Canvas {canvas_id} was not found")]
    CanvasNotFound {
        /// The missing Canvas.
        canvas_id: CanvasId,
    },
    /// The Action Session did not own the requested Canvas.
    #[error("Canvas {canvas_id} belongs to Session {expected}, but the Action targets {actual}")]
    CanvasSessionMismatch {
        /// The requested Canvas.
        canvas_id: CanvasId,
        /// The trusted owning Session.
        expected: SessionId,
        /// The Action Session.
        actual: SessionId,
    },
    /// The requested layer does not belong to the Canvas.
    #[error("Layer {layer_id} does not belong to Canvas {canvas_id}")]
    LayerNotFound {
        /// The target Canvas.
        canvas_id: CanvasId,
        /// The unknown layer.
        layer_id: LayerId,
    },
    /// The Canvas metadata source failed.
    #[error("Canvas catalog is unavailable")]
    CatalogFailed {
        /// The internal catalog failure.
        #[source]
        source: CanvasCatalogError,
    },
    /// The Stroke state source failed.
    #[error("Canvas state is unavailable")]
    StateReadFailed {
        /// The internal state read failure.
        #[source]
        source: CanvasStateReadError,
    },
    /// The command violated a Canvas domain rule.
    #[error("Canvas command violated a domain rule: {source}")]
    DomainViolation {
        /// The pure domain validation error.
        #[source]
        source: CanvasError,
    },
    /// Persisted or returned projection state was internally inconsistent.
    #[error("Canvas projection is corrupted")]
    ProjectionCorrupted {
        /// The deterministic projection failure.
        #[source]
        source: CanvasProjectionError,
    },
    /// A valid Canvas event payload could not be encoded.
    #[error("Canvas event payload encoding failed")]
    EncodingFailed {
        /// The Canvas payload conversion error.
        #[source]
        source: CanvasError,
    },
    /// No later chunk can be represented for this Stroke.
    #[error("Stroke {stroke_id} exhausted the chunk index range")]
    ChunkIndexOverflow {
        /// The affected Stroke.
        stroke_id: StrokeId,
    },
}

impl From<CanvasRuntimeError> for HandlerError {
    fn from(error: CanvasRuntimeError) -> Self {
        let message = match error {
            CanvasRuntimeError::CanvasNotFound { .. } => "Canvas was not found",
            CanvasRuntimeError::CanvasSessionMismatch { .. } => {
                "Canvas does not belong to the Action Session"
            }
            CanvasRuntimeError::LayerNotFound { .. } => "Canvas layer was not found",
            CanvasRuntimeError::CatalogFailed { .. } => "Canvas catalog is unavailable",
            CanvasRuntimeError::StateReadFailed { .. } => "Canvas state is unavailable",
            CanvasRuntimeError::DomainViolation { .. } => "Canvas command violated a domain rule",
            CanvasRuntimeError::ProjectionCorrupted { .. } => "Canvas projection is corrupted",
            CanvasRuntimeError::EncodingFailed { .. } => "Canvas event encoding failed",
            CanvasRuntimeError::ChunkIndexOverflow { .. } => "Canvas chunk index is exhausted",
        };
        Self::new(message)
    }
}
