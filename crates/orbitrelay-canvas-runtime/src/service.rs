//! Shared aggregate-aware command processing used by Canvas handlers.

use std::sync::Arc;

use orbitrelay_canvas::{
    CanvasDescriptor, CanvasError, CanvasId, CanvasPoint, CanvasProjectionError,
    StrokeAppendPayload, StrokeBeginPayload, StrokeCancelPayload, StrokeEndPayload, StrokeId,
    StrokeLifecycle, StrokeProjection, StrokeRemovePayload, STROKE_BEGAN_EVENT_TYPE,
    STROKE_CANCELLED_EVENT_TYPE, STROKE_COMPLETED_EVENT_TYPE, STROKE_POINTS_APPENDED_EVENT_TYPE,
    STROKE_REMOVED_EVENT_TYPE,
};
use orbitrelay_core::Metadata;
use orbitrelay_protocol::{Action, EventType, Payload};
use orbitrelay_runtime::EventDraft;

use crate::{CanvasCatalog, CanvasRuntimeError, CanvasStateReadError, CanvasStateReader};

/// Aggregate-aware Canvas command processor shared by all Stroke handlers.
pub struct CanvasCommandService {
    catalog: Arc<dyn CanvasCatalog>,
    state_reader: Arc<dyn CanvasStateReader>,
}

impl CanvasCommandService {
    /// Creates a service from trusted Canvas metadata and persisted-state ports.
    #[must_use]
    pub fn new(catalog: Arc<dyn CanvasCatalog>, state_reader: Arc<dyn CanvasStateReader>) -> Self {
        Self {
            catalog,
            state_reader,
        }
    }

    /// Processes a validated Stroke begin command.
    pub async fn begin(
        &self,
        action: &Action,
        payload: &StrokeBeginPayload,
    ) -> Result<Vec<EventDraft>, CanvasRuntimeError> {
        let descriptor = self.load_canvas(action, payload.canvas_id()).await?;
        self.validate_layer(&descriptor, payload.layer_id())?;
        validate_points(&descriptor, payload.points())?;

        let existing = self
            .load_stroke(action, payload.canvas_id(), payload.stroke_id())
            .await?;
        if let Some(existing) = existing {
            validate_projection_identity(
                &existing,
                action,
                payload.canvas_id(),
                payload.stroke_id(),
            )?;
            let same_begin = existing.layer_id() == payload.layer_id()
                && existing.tool() == payload.tool()
                && existing.style() == payload.style()
                && existing
                    .chunk(0)
                    .is_some_and(|chunk| chunk.points() == payload.points())
                && existing.creator_actor_id() == action.actor_id();
            if same_begin {
                return Ok(Vec::new());
            }
            return Err(domain(CanvasError::StrokeAlreadyExists {
                stroke_id: payload.stroke_id().clone(),
            }));
        }

        event_draft(STROKE_BEGAN_EVENT_TYPE, payload)
    }

    /// Processes a validated Stroke point-append command.
    pub async fn append(
        &self,
        action: &Action,
        payload: &StrokeAppendPayload,
    ) -> Result<Vec<EventDraft>, CanvasRuntimeError> {
        let descriptor = self.load_canvas(action, payload.canvas_id()).await?;
        let projection = self
            .load_required_stroke(action, payload.canvas_id(), payload.stroke_id())
            .await?;
        validate_projection_identity(
            &projection,
            action,
            payload.canvas_id(),
            payload.stroke_id(),
        )?;

        let last = projection.last_chunk_index();
        if payload.chunk_index() <= last {
            let chunk = projection.chunk(payload.chunk_index()).ok_or_else(|| {
                corrupted(CanvasProjectionError::InvalidHistoryChunk {
                    stroke_id: payload.stroke_id().clone(),
                    expected: payload.chunk_index(),
                    actual: last,
                })
            })?;
            if chunk.points() == payload.points() {
                return Ok(Vec::new());
            }
            return Err(domain(CanvasError::ChunkConflict {
                stroke_id: payload.stroke_id().clone(),
                chunk_index: payload.chunk_index(),
            }));
        }

        let expected = projection.next_chunk_index().ok_or_else(|| {
            CanvasRuntimeError::ChunkIndexOverflow {
                stroke_id: payload.stroke_id().clone(),
            }
        })?;
        if payload.chunk_index() > expected {
            return Err(domain(CanvasError::MissingChunk {
                stroke_id: payload.stroke_id().clone(),
                expected,
                actual: payload.chunk_index(),
            }));
        }
        if projection.lifecycle() != StrokeLifecycle::Active {
            return Err(invalid_state(
                projection.lifecycle(),
                StrokeLifecycle::Active,
            ));
        }
        validate_points(&descriptor, payload.points())?;

        event_draft(STROKE_POINTS_APPENDED_EVENT_TYPE, payload)
    }

    /// Processes a validated Stroke completion command.
    pub async fn end(
        &self,
        action: &Action,
        payload: &StrokeEndPayload,
    ) -> Result<Vec<EventDraft>, CanvasRuntimeError> {
        self.load_canvas(action, payload.canvas_id()).await?;
        let projection = self
            .load_required_stroke(action, payload.canvas_id(), payload.stroke_id())
            .await?;
        validate_projection_identity(
            &projection,
            action,
            payload.canvas_id(),
            payload.stroke_id(),
        )?;

        match projection.lifecycle() {
            StrokeLifecycle::Active => {
                validate_final_index("end", &projection, payload.final_chunk_index())?;
                event_draft(STROKE_COMPLETED_EVENT_TYPE, payload)
            }
            StrokeLifecycle::Completed | StrokeLifecycle::Removed => {
                validate_final_index("end", &projection, payload.final_chunk_index())?;
                Ok(Vec::new())
            }
            StrokeLifecycle::Cancelled => Err(invalid_state(
                StrokeLifecycle::Cancelled,
                StrokeLifecycle::Completed,
            )),
            state => Err(invalid_state(state, StrokeLifecycle::Completed)),
        }
    }

    /// Processes a validated Stroke cancellation command.
    pub async fn cancel(
        &self,
        action: &Action,
        payload: &StrokeCancelPayload,
    ) -> Result<Vec<EventDraft>, CanvasRuntimeError> {
        self.load_canvas(action, payload.canvas_id()).await?;
        let projection = self
            .load_required_stroke(action, payload.canvas_id(), payload.stroke_id())
            .await?;
        validate_projection_identity(
            &projection,
            action,
            payload.canvas_id(),
            payload.stroke_id(),
        )?;

        match projection.lifecycle() {
            StrokeLifecycle::Active => {
                validate_final_index("cancel", &projection, payload.final_chunk_index())?;
                event_draft(STROKE_CANCELLED_EVENT_TYPE, payload)
            }
            StrokeLifecycle::Cancelled => {
                validate_final_index("cancel", &projection, payload.final_chunk_index())?;
                Ok(Vec::new())
            }
            StrokeLifecycle::Completed | StrokeLifecycle::Removed => Err(invalid_state(
                projection.lifecycle(),
                StrokeLifecycle::Cancelled,
            )),
            state => Err(invalid_state(state, StrokeLifecycle::Cancelled)),
        }
    }

    /// Processes a validated completed-Stroke removal command.
    pub async fn remove(
        &self,
        action: &Action,
        payload: &StrokeRemovePayload,
    ) -> Result<Vec<EventDraft>, CanvasRuntimeError> {
        self.load_canvas(action, payload.canvas_id()).await?;
        let projection = self
            .load_required_stroke(action, payload.canvas_id(), payload.stroke_id())
            .await?;
        validate_projection_identity(
            &projection,
            action,
            payload.canvas_id(),
            payload.stroke_id(),
        )?;

        match projection.lifecycle() {
            StrokeLifecycle::Completed => event_draft(STROKE_REMOVED_EVENT_TYPE, payload),
            StrokeLifecycle::Removed => Ok(Vec::new()),
            StrokeLifecycle::Active | StrokeLifecycle::Cancelled => Err(invalid_state(
                projection.lifecycle(),
                StrokeLifecycle::Removed,
            )),
            state => Err(invalid_state(state, StrokeLifecycle::Removed)),
        }
    }

    async fn load_canvas(
        &self,
        action: &Action,
        canvas_id: &CanvasId,
    ) -> Result<CanvasDescriptor, CanvasRuntimeError> {
        let descriptor = self
            .catalog
            .get_canvas(canvas_id)
            .await
            .map_err(|source| CanvasRuntimeError::CatalogFailed { source })?
            .ok_or_else(|| CanvasRuntimeError::CanvasNotFound {
                canvas_id: canvas_id.clone(),
            })?;
        if descriptor.session_id() != action.session_id() {
            return Err(CanvasRuntimeError::CanvasSessionMismatch {
                canvas_id: canvas_id.clone(),
                expected: descriptor.session_id().clone(),
                actual: action.session_id().clone(),
            });
        }
        Ok(descriptor)
    }

    async fn load_stroke(
        &self,
        action: &Action,
        canvas_id: &CanvasId,
        stroke_id: &StrokeId,
    ) -> Result<Option<StrokeProjection>, CanvasRuntimeError> {
        self.state_reader
            .load_stroke(action.session_id(), canvas_id, stroke_id)
            .await
            .map_err(map_state_read_error)
    }

    async fn load_required_stroke(
        &self,
        action: &Action,
        canvas_id: &CanvasId,
        stroke_id: &StrokeId,
    ) -> Result<StrokeProjection, CanvasRuntimeError> {
        self.load_stroke(action, canvas_id, stroke_id)
            .await?
            .ok_or_else(|| {
                domain(CanvasError::StrokeNotFound {
                    stroke_id: stroke_id.clone(),
                })
            })
    }

    fn validate_layer(
        &self,
        descriptor: &CanvasDescriptor,
        layer_id: &orbitrelay_canvas::LayerId,
    ) -> Result<(), CanvasRuntimeError> {
        if descriptor.contains_layer(layer_id) {
            Ok(())
        } else {
            Err(CanvasRuntimeError::LayerNotFound {
                canvas_id: descriptor.canvas_id().clone(),
                layer_id: layer_id.clone(),
            })
        }
    }
}

fn validate_projection_identity(
    projection: &StrokeProjection,
    action: &Action,
    canvas_id: &CanvasId,
    stroke_id: &StrokeId,
) -> Result<(), CanvasRuntimeError> {
    if projection.session_id() != action.session_id() {
        return Err(corrupted(CanvasProjectionError::SessionMismatch {
            expected: action.session_id().clone(),
            actual: projection.session_id().clone(),
        }));
    }
    if projection.canvas_id() != canvas_id {
        return Err(corrupted(CanvasProjectionError::CanvasMismatch {
            expected: canvas_id.clone(),
            actual: projection.canvas_id().clone(),
        }));
    }
    if projection.stroke_id() != stroke_id {
        return Err(corrupted(CanvasProjectionError::StrokeMismatch {
            expected: stroke_id.clone(),
            actual: projection.stroke_id().clone(),
        }));
    }
    Ok(())
}

fn validate_points(
    descriptor: &CanvasDescriptor,
    points: &[CanvasPoint],
) -> Result<(), CanvasRuntimeError> {
    for point in points {
        descriptor.space().validate_point(point).map_err(domain)?;
    }
    Ok(())
}

fn validate_final_index(
    kind: &'static str,
    projection: &StrokeProjection,
    final_chunk_index: u64,
) -> Result<(), CanvasRuntimeError> {
    if final_chunk_index == projection.last_chunk_index() {
        Ok(())
    } else {
        Err(domain(CanvasError::InvalidChunkIndex {
            kind,
            chunk_index: final_chunk_index,
        }))
    }
}

fn invalid_state(from: StrokeLifecycle, to: StrokeLifecycle) -> CanvasRuntimeError {
    domain(CanvasError::InvalidStrokeState { from, to })
}

fn domain(source: CanvasError) -> CanvasRuntimeError {
    CanvasRuntimeError::DomainViolation { source }
}

fn corrupted(source: CanvasProjectionError) -> CanvasRuntimeError {
    CanvasRuntimeError::ProjectionCorrupted { source }
}

fn map_state_read_error(source: CanvasStateReadError) -> CanvasRuntimeError {
    match source {
        CanvasStateReadError::ProjectionCorrupted { source } => corrupted(source),
        source => CanvasRuntimeError::StateReadFailed { source },
    }
}

fn event_draft<T>(
    event_type: &'static str,
    payload: &T,
) -> Result<Vec<EventDraft>, CanvasRuntimeError>
where
    for<'a> Payload: TryFrom<&'a T, Error = CanvasError>,
{
    let payload = Payload::try_from(payload)
        .map_err(|source| CanvasRuntimeError::EncodingFailed { source })?;
    Ok(vec![EventDraft::new(
        EventType::new(event_type),
        payload,
        Metadata::new(),
    )])
}
