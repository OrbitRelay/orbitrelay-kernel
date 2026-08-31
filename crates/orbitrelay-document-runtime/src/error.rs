//! Stable error boundaries for Document composition and discovery reads.

use thiserror::Error;

use orbitrelay_asset::AssetId;
use orbitrelay_canvas::{CanvasError, CanvasId};
use orbitrelay_document::{DocumentError, DocumentId};

/// A failure while querying a Document catalog.
///
/// The diagnostic is intended for operator logs. Its public display text is
/// deliberately stable and does not expose a backend implementation detail.
#[derive(Debug, Error)]
#[error("document catalog query failed")]
pub struct DocumentCatalogError {
    detail: String,
}

impl DocumentCatalogError {
    /// Creates a catalog failure with operator-facing detail.
    #[must_use]
    pub fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }

    /// Returns operator-facing detail for diagnostics and logging.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

/// A failure while building a complete Document/Canvas composition.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DocumentCompositionError {
    /// The composition input does not contain a usable page list or value.
    #[error("invalid document composition input: {reason}")]
    InvalidInput {
        /// A stable description of the invalid input.
        reason: &'static str,
    },

    /// A source page index is not the expected zero-based contiguous value.
    #[error("source page index {actual} is out of order; expected {expected}")]
    InvalidPageSequence {
        /// The expected index at this vector position.
        expected: u32,
        /// The supplied source index.
        actual: u32,
    },

    /// The Canvas domain rejected a generated descriptor.
    #[error("generated Canvas descriptor is invalid")]
    CanvasDescriptorFailed {
        /// The underlying Canvas domain failure.
        #[source]
        source: CanvasError,
    },

    /// The Document domain rejected a generated descriptor.
    #[error("generated Document descriptor is invalid")]
    DocumentDescriptorFailed {
        /// The underlying Document domain failure.
        #[source]
        source: DocumentError,
    },

    /// The completed bundle did not satisfy its cross-domain invariants.
    #[error("Document composition invariant failed: {reason}")]
    CompositionInvariantViolation {
        /// A stable description of the violated invariant.
        reason: &'static str,
    },
}

/// A failure while assembling a read model from Document, Asset, and Canvas
/// catalogs.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DocumentReadError {
    /// The requested Document is not present in the catalog.
    #[error("Document {document_id} was not found")]
    DocumentNotFound {
        /// The missing Document identity.
        document_id: DocumentId,
    },

    /// A Document references an Asset that is not present.
    #[error("source Asset {asset_id} was not found")]
    AssetNotFound {
        /// The missing Asset identity.
        asset_id: AssetId,
    },

    /// A Document page references a Canvas that is not present.
    #[error("overlay Canvas {canvas_id} was not found")]
    CanvasNotFound {
        /// The missing Canvas identity.
        canvas_id: CanvasId,
    },

    /// One of the read catalogs failed without exposing backend diagnostics.
    #[error("{catalog} catalog is unavailable")]
    CatalogUnavailable {
        /// The logical catalog that failed.
        catalog: &'static str,
    },

    /// Catalog values disagree and cannot form a trusted complete view.
    #[error("Document read model is inconsistent: {reason}")]
    InconsistentReadModel {
        /// A stable description of the violated cross-catalog invariant.
        reason: &'static str,
    },

    /// An internal page count could not be represented by the read model type.
    #[error("Document page count cannot be represented")]
    PageCountOverflow,
}
