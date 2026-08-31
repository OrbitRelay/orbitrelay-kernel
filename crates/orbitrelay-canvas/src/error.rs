//! Canvas domain validation failures.

use thiserror::Error;

use orbitrelay_protocol::{EventType, SessionId};

use crate::{CanvasId, LayerId, StrokeId, StrokeLifecycle};

/// Errors produced by pure Canvas domain parsing and validation.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum CanvasError {
    /// A generic protocol payload did not match the expected strict schema.
    #[error("invalid Canvas payload: {detail}")]
    InvalidPayload {
        /// Safe diagnostic detail.
        detail: String,
    },
    /// A coordinate was non-finite or outside its Canvas space.
    #[error("invalid Canvas coordinate `{coordinate}`")]
    InvalidCoordinate {
        /// The invalid coordinate or range component.
        coordinate: &'static str,
    },
    /// A Canvas dimension was not finite and positive.
    #[error("invalid Canvas space dimension `{dimension}`")]
    InvalidCanvasSpace {
        /// The invalid Canvas dimension.
        dimension: &'static str,
    },
    /// A Stroke style field was invalid.
    #[error("invalid Stroke style field `{field}`")]
    InvalidStyle {
        /// The invalid style field.
        field: &'static str,
    },
    /// A Canvas descriptor did not contain any layers.
    #[error("Canvas descriptor must contain at least one layer")]
    EmptyLayerSet,
    /// A Canvas descriptor's default layer was absent from its layer set.
    #[error("default layer {default_layer_id} does not belong to the Canvas descriptor")]
    DefaultLayerNotFound {
        /// The invalid default layer.
        default_layer_id: LayerId,
    },
    /// A referenced Stroke does not exist.
    #[error("Stroke {stroke_id} was not found")]
    StrokeNotFound {
        /// The missing Stroke identifier.
        stroke_id: StrokeId,
    },
    /// A Stroke identifier is already in use.
    #[error("Stroke {stroke_id} already exists")]
    StrokeAlreadyExists {
        /// The conflicting Stroke identifier.
        stroke_id: StrokeId,
    },
    /// A requested Stroke lifecycle transition is not legal.
    #[error("Stroke cannot transition from {from:?} to {to:?}")]
    InvalidStrokeState {
        /// The current lifecycle state.
        from: StrokeLifecycle,
        /// The requested lifecycle state.
        to: StrokeLifecycle,
    },
    /// A chunk index violates the structural rule for its action.
    #[error("invalid {kind} chunk index {chunk_index}")]
    InvalidChunkIndex {
        /// The kind of Canvas command carrying the index.
        kind: &'static str,
        /// The invalid chunk index.
        chunk_index: u64,
    },
    /// An idempotency key was reused with different chunk content.
    #[error("Stroke {stroke_id} chunk {chunk_index} conflicts with existing content")]
    ChunkConflict {
        /// The affected Stroke.
        stroke_id: StrokeId,
        /// The conflicting chunk index.
        chunk_index: u64,
    },
    /// A chunk arrived after a gap in a Stroke sequence.
    #[error("Stroke {stroke_id} expected chunk {expected}, received {actual}")]
    MissingChunk {
        /// The affected Stroke.
        stroke_id: StrokeId,
        /// The next expected chunk index.
        expected: u64,
        /// The received chunk index.
        actual: u64,
    },
    /// A point batch was empty or exceeded the protocol limit.
    #[error("invalid point count {actual}; expected 1 through {maximum}")]
    InvalidPointCount {
        /// The supplied number of points.
        actual: usize,
        /// The maximum points allowed in one chunk.
        maximum: usize,
    },
}

/// Errors indicating that persisted Canvas event history is inconsistent.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum CanvasProjectionError {
    /// A non-Canvas event was passed to the Stroke projector.
    #[error("event type `{event_type}` is not a Canvas event")]
    UnexpectedEvent {
        /// The unsupported event type.
        event_type: EventType,
    },
    /// A recognized Canvas event carried an invalid strict payload.
    #[error("Canvas event `{event_type}` contains an invalid payload")]
    InvalidEventPayload {
        /// The recognized Canvas event type.
        event_type: EventType,
        /// The payload validation failure.
        #[source]
        source: CanvasError,
    },
    /// A second begin fact targeted an already-created projection.
    #[error("Stroke {stroke_id} history contains more than one begin fact")]
    ProjectionAlreadyExists {
        /// The already-created Stroke.
        stroke_id: StrokeId,
    },
    /// A non-begin fact appeared before its Stroke begin fact.
    #[error("event `{event_type}` requires an existing Stroke projection")]
    ProjectionMissing {
        /// The event that required prior state.
        event_type: EventType,
    },
    /// A Stroke history crossed session boundaries.
    #[error("Stroke history session mismatch: expected {expected}, received {actual}")]
    SessionMismatch {
        /// Session recorded by the begin fact.
        expected: SessionId,
        /// Session recorded by the later fact.
        actual: SessionId,
    },
    /// A Stroke history crossed Canvas boundaries.
    #[error("Stroke history Canvas mismatch: expected {expected}, received {actual}")]
    CanvasMismatch {
        /// Canvas recorded by the begin fact.
        expected: CanvasId,
        /// Canvas recorded by the later fact.
        actual: CanvasId,
    },
    /// A fact was applied to a projection for another Stroke.
    #[error("Stroke history identifier mismatch: expected {expected}, received {actual}")]
    StrokeMismatch {
        /// Stroke represented by the projection.
        expected: StrokeId,
        /// Stroke named by the later fact.
        actual: StrokeId,
    },
    /// An event was not legal for the persisted lifecycle state.
    #[error("event `{event_type}` is invalid for Stroke {stroke_id} in state {lifecycle:?}")]
    InvalidHistoryState {
        /// The affected Stroke.
        stroke_id: StrokeId,
        /// The state before applying the invalid event.
        lifecycle: StrokeLifecycle,
        /// The event that could not be applied.
        event_type: EventType,
    },
    /// A persisted chunk or terminal index broke contiguous history.
    #[error("Stroke {stroke_id} history expected chunk {expected}, received {actual}")]
    InvalidHistoryChunk {
        /// The affected Stroke.
        stroke_id: StrokeId,
        /// The required chunk index.
        expected: u64,
        /// The persisted chunk index.
        actual: u64,
    },
    /// A persisted Stroke exhausted the chunk index range.
    #[error("Stroke {stroke_id} chunk index overflowed")]
    ChunkIndexOverflow {
        /// The affected Stroke.
        stroke_id: StrokeId,
    },
}
