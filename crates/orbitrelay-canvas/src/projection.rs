//! Deterministic Stroke state derived exclusively from Canvas events.

use orbitrelay_core::Timestamp;
use orbitrelay_protocol::{ActorId, Event, EventType, SessionId};

use crate::{
    CanvasError, CanvasEventData, CanvasId, CanvasPoint, CanvasProjectionError, LayerId, StrokeId,
    StrokeLifecycle, StrokeStyle, StrokeTool, MAX_POINTS_PER_CHUNK,
};

/// One contiguous, ordered batch of points within a Stroke.
#[derive(Clone, Debug, PartialEq)]
pub struct StrokeChunk {
    index: u64,
    points: Vec<CanvasPoint>,
}

impl StrokeChunk {
    /// Creates a chunk with the protocol point-count limit applied.
    pub fn new(
        index: u64,
        points: impl IntoIterator<Item = CanvasPoint>,
    ) -> Result<Self, CanvasError> {
        let points = points.into_iter().collect::<Vec<_>>();
        if !(1..=MAX_POINTS_PER_CHUNK).contains(&points.len()) {
            return Err(CanvasError::InvalidPointCount {
                actual: points.len(),
                maximum: MAX_POINTS_PER_CHUNK,
            });
        }
        Ok(Self { index, points })
    }

    /// Returns this chunk's contiguous Stroke index.
    #[must_use]
    pub const fn index(&self) -> u64 {
        self.index
    }

    /// Returns points in their persisted drawing order.
    #[must_use]
    pub fn points(&self) -> &[CanvasPoint] {
        &self.points
    }
}

/// Rebuildable current state of one Stroke aggregate.
#[derive(Clone, Debug, PartialEq)]
pub struct StrokeProjection {
    stroke_id: StrokeId,
    canvas_id: CanvasId,
    layer_id: LayerId,
    session_id: SessionId,
    creator_actor_id: ActorId,
    tool: StrokeTool,
    style: StrokeStyle,
    chunks: Vec<StrokeChunk>,
    lifecycle: StrokeLifecycle,
    created_at: Timestamp,
}

impl StrokeProjection {
    /// Returns the stable Stroke identifier.
    #[must_use]
    pub const fn stroke_id(&self) -> &StrokeId {
        &self.stroke_id
    }

    /// Returns the Canvas containing this Stroke.
    #[must_use]
    pub const fn canvas_id(&self) -> &CanvasId {
        &self.canvas_id
    }

    /// Returns the layer selected by the begin fact.
    #[must_use]
    pub const fn layer_id(&self) -> &LayerId {
        &self.layer_id
    }

    /// Returns the session recorded by the begin fact.
    #[must_use]
    pub const fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    /// Returns the actor recorded by the begin fact.
    #[must_use]
    pub const fn creator_actor_id(&self) -> &ActorId {
        &self.creator_actor_id
    }

    /// Returns the Stroke drawing tool.
    #[must_use]
    pub const fn tool(&self) -> StrokeTool {
        self.tool
    }

    /// Returns the immutable logical Stroke style.
    #[must_use]
    pub const fn style(&self) -> &StrokeStyle {
        &self.style
    }

    /// Returns all contiguous point chunks in index order.
    #[must_use]
    pub fn chunks(&self) -> &[StrokeChunk] {
        &self.chunks
    }

    /// Returns a chunk by its logical index.
    #[must_use]
    pub fn chunk(&self, index: u64) -> Option<&StrokeChunk> {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.chunks.get(index))
            .filter(|chunk| chunk.index == index)
    }

    /// Returns the final contiguous chunk index.
    #[must_use]
    pub fn last_chunk_index(&self) -> u64 {
        self.chunks
            .last()
            .expect("a Stroke projection always contains begin chunk zero")
            .index
    }

    /// Returns the next contiguous chunk index, or `None` on overflow.
    #[must_use]
    pub fn next_chunk_index(&self) -> Option<u64> {
        self.last_chunk_index().checked_add(1)
    }

    /// Returns the current lifecycle derived from terminal facts.
    #[must_use]
    pub const fn lifecycle(&self) -> StrokeLifecycle {
        self.lifecycle
    }

    /// Returns the begin fact timestamp.
    #[must_use]
    pub const fn created_at(&self) -> &Timestamp {
        &self.created_at
    }
}

/// Applies recognized Canvas events to a single Stroke projection.
#[derive(Clone, Copy, Debug, Default)]
pub struct StrokeProjector;

impl StrokeProjector {
    /// Applies one recognized Canvas event in persisted order.
    ///
    /// Non-Canvas events return [`CanvasProjectionError::UnexpectedEvent`].
    /// Callers that scan mixed event streams should classify event types before
    /// invoking this function.
    pub fn apply(
        projection: Option<StrokeProjection>,
        event: &Event,
    ) -> Result<StrokeProjection, CanvasProjectionError> {
        let event_data = CanvasEventData::try_from(event)?;
        match event_data {
            CanvasEventData::StrokeBegan(payload) => {
                if let Some(existing) = projection {
                    return Err(CanvasProjectionError::ProjectionAlreadyExists {
                        stroke_id: existing.stroke_id,
                    });
                }
                let chunk =
                    StrokeChunk::new(payload.chunk_index(), payload.points().iter().copied())
                        .map_err(|source| CanvasProjectionError::InvalidEventPayload {
                            event_type: event.event_type().clone(),
                            source,
                        })?;
                Ok(StrokeProjection {
                    stroke_id: payload.stroke_id().clone(),
                    canvas_id: payload.canvas_id().clone(),
                    layer_id: payload.layer_id().clone(),
                    session_id: event.session_id().clone(),
                    creator_actor_id: event.actor_id().clone(),
                    tool: payload.tool(),
                    style: payload.style().clone(),
                    chunks: vec![chunk],
                    lifecycle: StrokeLifecycle::Active,
                    created_at: event.occurred_at().clone(),
                })
            }
            CanvasEventData::StrokePointsAppended(payload) => {
                let mut projection = require_projection(projection, event.event_type())?;
                validate_identity(&projection, event, payload.canvas_id(), payload.stroke_id())?;
                require_lifecycle(&projection, StrokeLifecycle::Active, event.event_type())?;
                let expected = projection.next_chunk_index().ok_or_else(|| {
                    CanvasProjectionError::ChunkIndexOverflow {
                        stroke_id: projection.stroke_id.clone(),
                    }
                })?;
                if payload.chunk_index() != expected {
                    return Err(CanvasProjectionError::InvalidHistoryChunk {
                        stroke_id: projection.stroke_id.clone(),
                        expected,
                        actual: payload.chunk_index(),
                    });
                }
                let chunk =
                    StrokeChunk::new(payload.chunk_index(), payload.points().iter().copied())
                        .map_err(|source| CanvasProjectionError::InvalidEventPayload {
                            event_type: event.event_type().clone(),
                            source,
                        })?;
                projection.chunks.push(chunk);
                Ok(projection)
            }
            CanvasEventData::StrokeCompleted(payload) => apply_terminal(
                projection,
                event,
                payload.canvas_id(),
                payload.stroke_id(),
                payload.final_chunk_index(),
                StrokeLifecycle::Completed,
            ),
            CanvasEventData::StrokeCancelled(payload) => apply_terminal(
                projection,
                event,
                payload.canvas_id(),
                payload.stroke_id(),
                payload.final_chunk_index(),
                StrokeLifecycle::Cancelled,
            ),
            CanvasEventData::StrokeRemoved(payload) => {
                let mut projection = require_projection(projection, event.event_type())?;
                validate_identity(&projection, event, payload.canvas_id(), payload.stroke_id())?;
                require_lifecycle(&projection, StrokeLifecycle::Completed, event.event_type())?;
                projection.lifecycle = StrokeLifecycle::Removed;
                Ok(projection)
            }
        }
    }
}

fn require_projection(
    projection: Option<StrokeProjection>,
    event_type: &EventType,
) -> Result<StrokeProjection, CanvasProjectionError> {
    projection.ok_or_else(|| CanvasProjectionError::ProjectionMissing {
        event_type: event_type.clone(),
    })
}

fn validate_identity(
    projection: &StrokeProjection,
    event: &Event,
    canvas_id: &CanvasId,
    stroke_id: &StrokeId,
) -> Result<(), CanvasProjectionError> {
    if event.session_id() != projection.session_id() {
        return Err(CanvasProjectionError::SessionMismatch {
            expected: projection.session_id().clone(),
            actual: event.session_id().clone(),
        });
    }
    if canvas_id != projection.canvas_id() {
        return Err(CanvasProjectionError::CanvasMismatch {
            expected: projection.canvas_id().clone(),
            actual: canvas_id.clone(),
        });
    }
    if stroke_id != projection.stroke_id() {
        return Err(CanvasProjectionError::StrokeMismatch {
            expected: projection.stroke_id().clone(),
            actual: stroke_id.clone(),
        });
    }
    Ok(())
}

fn require_lifecycle(
    projection: &StrokeProjection,
    expected: StrokeLifecycle,
    event_type: &EventType,
) -> Result<(), CanvasProjectionError> {
    if projection.lifecycle() == expected {
        Ok(())
    } else {
        Err(CanvasProjectionError::InvalidHistoryState {
            stroke_id: projection.stroke_id().clone(),
            lifecycle: projection.lifecycle(),
            event_type: event_type.clone(),
        })
    }
}

fn apply_terminal(
    projection: Option<StrokeProjection>,
    event: &Event,
    canvas_id: &CanvasId,
    stroke_id: &StrokeId,
    final_chunk_index: u64,
    target: StrokeLifecycle,
) -> Result<StrokeProjection, CanvasProjectionError> {
    let mut projection = require_projection(projection, event.event_type())?;
    validate_identity(&projection, event, canvas_id, stroke_id)?;
    require_lifecycle(&projection, StrokeLifecycle::Active, event.event_type())?;
    let expected = projection.last_chunk_index();
    if final_chunk_index != expected {
        return Err(CanvasProjectionError::InvalidHistoryChunk {
            stroke_id: projection.stroke_id.clone(),
            expected,
            actual: final_chunk_index,
        });
    }
    projection.lifecycle = target;
    Ok(projection)
}

#[cfg(test)]
mod tests {
    use orbitrelay_core::{Metadata, Timestamp};
    use orbitrelay_protocol::{ActionId, ActorId, Event, EventId, EventType, Payload, SessionId};

    use super::{StrokeProjection, StrokeProjector};
    use crate::{
        CanvasError, CanvasId, CanvasPoint, CanvasProjectionError, LayerId, RgbaColor,
        StrokeAppendPayload, StrokeBeginPayload, StrokeCancelPayload, StrokeEndPayload, StrokeId,
        StrokeLifecycle, StrokeRemovePayload, StrokeStyle, StrokeTool, STROKE_BEGAN_EVENT_TYPE,
        STROKE_CANCELLED_EVENT_TYPE, STROKE_COMPLETED_EVENT_TYPE,
        STROKE_POINTS_APPENDED_EVENT_TYPE, STROKE_REMOVED_EVENT_TYPE,
    };

    struct Fixture {
        session_id: SessionId,
        actor_id: ActorId,
        canvas_id: CanvasId,
        layer_id: LayerId,
        stroke_id: StrokeId,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                session_id: SessionId::new(),
                actor_id: ActorId::new(),
                canvas_id: CanvasId::new(),
                layer_id: LayerId::new(),
                stroke_id: StrokeId::new(),
            }
        }

        fn began(&self) -> Event {
            event(
                self.session_id.clone(),
                self.actor_id.clone(),
                STROKE_BEGAN_EVENT_TYPE,
                StrokeBeginPayload::new(
                    self.canvas_id.clone(),
                    self.layer_id.clone(),
                    self.stroke_id.clone(),
                    StrokeTool::Pen,
                    style(),
                    0,
                    [point(1.0)],
                )
                .expect("begin payload should be valid"),
            )
        }

        fn projection(&self) -> StrokeProjection {
            StrokeProjector::apply(None, &self.began()).expect("begin should project")
        }
    }

    fn point(value: f64) -> CanvasPoint {
        CanvasPoint::new(value, value).expect("point should be finite")
    }

    fn style() -> StrokeStyle {
        StrokeStyle::new(2.0, RgbaColor::new(1, 2, 3, 255)).expect("style should be valid")
    }

    fn event<T>(session_id: SessionId, actor_id: ActorId, event_type: &str, payload: T) -> Event
    where
        Payload: TryFrom<T, Error = CanvasError>,
    {
        Event::new(
            EventId::new(),
            session_id,
            actor_id,
            ActionId::new(),
            EventType::new(event_type),
            Timestamp::from_unix_timestamp(1_700_000_000).expect("timestamp should be valid"),
            Payload::try_from(payload).expect("payload should encode"),
            Metadata::new(),
        )
    }

    #[test]
    fn began_creates_active_projection_from_event_metadata() {
        let fixture = Fixture::new();
        let event = fixture.began();
        let projection = StrokeProjector::apply(None, &event).expect("begin should project");

        assert_eq!(projection.stroke_id(), &fixture.stroke_id);
        assert_eq!(projection.canvas_id(), &fixture.canvas_id);
        assert_eq!(projection.layer_id(), &fixture.layer_id);
        assert_eq!(projection.session_id(), &fixture.session_id);
        assert_eq!(projection.creator_actor_id(), &fixture.actor_id);
        assert_eq!(projection.created_at(), event.occurred_at());
        assert_eq!(projection.lifecycle(), StrokeLifecycle::Active);
        assert_eq!(projection.last_chunk_index(), 0);
        assert_eq!(
            projection.chunk(0).expect("chunk zero").points(),
            &[point(1.0)]
        );
    }

    #[test]
    fn append_is_strictly_contiguous_and_rejects_duplicates_or_gaps() {
        let fixture = Fixture::new();
        let projection = fixture.projection();
        let append = event(
            fixture.session_id.clone(),
            fixture.actor_id.clone(),
            STROKE_POINTS_APPENDED_EVENT_TYPE,
            StrokeAppendPayload::new(
                fixture.canvas_id.clone(),
                fixture.stroke_id.clone(),
                1,
                [point(2.0)],
            )
            .expect("append payload should be valid"),
        );
        let projection =
            StrokeProjector::apply(Some(projection), &append).expect("append should project");
        assert_eq!(projection.last_chunk_index(), 1);

        assert!(matches!(
            StrokeProjector::apply(Some(projection.clone()), &append),
            Err(CanvasProjectionError::InvalidHistoryChunk {
                expected: 2,
                actual: 1,
                ..
            })
        ));

        let gap = event(
            fixture.session_id.clone(),
            fixture.actor_id.clone(),
            STROKE_POINTS_APPENDED_EVENT_TYPE,
            StrokeAppendPayload::new(
                fixture.canvas_id.clone(),
                fixture.stroke_id.clone(),
                3,
                [point(3.0)],
            )
            .expect("append payload should be valid"),
        );
        assert!(matches!(
            StrokeProjector::apply(Some(projection), &gap),
            Err(CanvasProjectionError::InvalidHistoryChunk {
                expected: 2,
                actual: 3,
                ..
            })
        ));
    }

    #[test]
    fn completes_cancels_and_removes_only_in_legal_order() {
        let completed_fixture = Fixture::new();
        let completed = event(
            completed_fixture.session_id.clone(),
            completed_fixture.actor_id.clone(),
            STROKE_COMPLETED_EVENT_TYPE,
            StrokeEndPayload::new(
                completed_fixture.canvas_id.clone(),
                completed_fixture.stroke_id.clone(),
                0,
            ),
        );
        let projection = StrokeProjector::apply(Some(completed_fixture.projection()), &completed)
            .expect("active Stroke should complete");
        assert_eq!(projection.lifecycle(), StrokeLifecycle::Completed);

        let removed = event(
            completed_fixture.session_id.clone(),
            completed_fixture.actor_id.clone(),
            STROKE_REMOVED_EVENT_TYPE,
            StrokeRemovePayload::new(
                completed_fixture.canvas_id.clone(),
                completed_fixture.stroke_id.clone(),
            ),
        );
        let projection = StrokeProjector::apply(Some(projection), &removed)
            .expect("completed Stroke should be removed");
        assert_eq!(projection.lifecycle(), StrokeLifecycle::Removed);
        assert_eq!(projection.chunks().len(), 1);

        let cancelled_fixture = Fixture::new();
        let cancelled = event(
            cancelled_fixture.session_id.clone(),
            cancelled_fixture.actor_id.clone(),
            STROKE_CANCELLED_EVENT_TYPE,
            StrokeCancelPayload::new(
                cancelled_fixture.canvas_id.clone(),
                cancelled_fixture.stroke_id.clone(),
                0,
            ),
        );
        let cancelled_projection =
            StrokeProjector::apply(Some(cancelled_fixture.projection()), &cancelled)
                .expect("active Stroke should cancel");
        assert_eq!(cancelled_projection.lifecycle(), StrokeLifecycle::Cancelled);
        let remove_cancelled = event(
            cancelled_fixture.session_id.clone(),
            cancelled_fixture.actor_id.clone(),
            STROKE_REMOVED_EVENT_TYPE,
            StrokeRemovePayload::new(
                cancelled_fixture.canvas_id.clone(),
                cancelled_fixture.stroke_id.clone(),
            ),
        );
        assert!(matches!(
            StrokeProjector::apply(Some(cancelled_projection), &remove_cancelled),
            Err(CanvasProjectionError::InvalidHistoryState { .. })
        ));
    }

    #[test]
    fn repeated_begin_and_terminal_mismatch_are_history_errors() {
        let fixture = Fixture::new();
        let began = fixture.began();
        let projection = StrokeProjector::apply(None, &began).expect("begin should project");
        assert!(matches!(
            StrokeProjector::apply(Some(projection.clone()), &began),
            Err(CanvasProjectionError::ProjectionAlreadyExists { .. })
        ));

        let wrong_final = event(
            fixture.session_id.clone(),
            fixture.actor_id.clone(),
            STROKE_COMPLETED_EVENT_TYPE,
            StrokeEndPayload::new(fixture.canvas_id.clone(), fixture.stroke_id.clone(), 4),
        );
        assert!(matches!(
            StrokeProjector::apply(Some(projection), &wrong_final),
            Err(CanvasProjectionError::InvalidHistoryChunk { .. })
        ));
    }

    #[test]
    fn rejects_session_canvas_and_stroke_mismatches() {
        let fixture = Fixture::new();
        let projection = fixture.projection();

        let session_mismatch = event(
            SessionId::new(),
            fixture.actor_id.clone(),
            STROKE_POINTS_APPENDED_EVENT_TYPE,
            StrokeAppendPayload::new(
                fixture.canvas_id.clone(),
                fixture.stroke_id.clone(),
                1,
                [point(2.0)],
            )
            .expect("append payload should be valid"),
        );
        assert!(matches!(
            StrokeProjector::apply(Some(projection.clone()), &session_mismatch),
            Err(CanvasProjectionError::SessionMismatch { .. })
        ));

        let canvas_mismatch = event(
            fixture.session_id.clone(),
            fixture.actor_id.clone(),
            STROKE_POINTS_APPENDED_EVENT_TYPE,
            StrokeAppendPayload::new(CanvasId::new(), fixture.stroke_id.clone(), 1, [point(2.0)])
                .expect("append payload should be valid"),
        );
        assert!(matches!(
            StrokeProjector::apply(Some(projection.clone()), &canvas_mismatch),
            Err(CanvasProjectionError::CanvasMismatch { .. })
        ));

        let stroke_mismatch = event(
            fixture.session_id.clone(),
            fixture.actor_id.clone(),
            STROKE_POINTS_APPENDED_EVENT_TYPE,
            StrokeAppendPayload::new(fixture.canvas_id.clone(), StrokeId::new(), 1, [point(2.0)])
                .expect("append payload should be valid"),
        );
        assert!(matches!(
            StrokeProjector::apply(Some(projection), &stroke_mismatch),
            Err(CanvasProjectionError::StrokeMismatch { .. })
        ));
    }

    #[test]
    fn detects_next_chunk_overflow_without_wrapping() {
        let fixture = Fixture::new();
        let mut projection = fixture.projection();
        projection
            .chunks
            .last_mut()
            .expect("projection has chunk zero")
            .index = u64::MAX;

        assert_eq!(projection.next_chunk_index(), None);
    }

    #[test]
    fn append_requires_existing_active_projection() {
        let fixture = Fixture::new();
        let append = event(
            fixture.session_id.clone(),
            fixture.actor_id.clone(),
            STROKE_POINTS_APPENDED_EVENT_TYPE,
            StrokeAppendPayload::new(
                fixture.canvas_id.clone(),
                fixture.stroke_id.clone(),
                1,
                [point(2.0)],
            )
            .expect("append payload should be valid"),
        );
        assert!(matches!(
            StrokeProjector::apply(None, &append),
            Err(CanvasProjectionError::ProjectionMissing { .. })
        ));

        let completed = event(
            fixture.session_id.clone(),
            fixture.actor_id.clone(),
            STROKE_COMPLETED_EVENT_TYPE,
            StrokeEndPayload::new(fixture.canvas_id.clone(), fixture.stroke_id.clone(), 0),
        );
        let completed_projection = StrokeProjector::apply(Some(fixture.projection()), &completed)
            .expect("completion should project");
        assert!(matches!(
            StrokeProjector::apply(Some(completed_projection), &append),
            Err(CanvasProjectionError::InvalidHistoryState { .. })
        ));
    }
}
