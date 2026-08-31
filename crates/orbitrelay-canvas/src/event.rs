//! Stable Canvas event type names.

use orbitrelay_protocol::{Event, EventType};

use crate::{
    CanvasProjectionError, StrokeAppendPayload, StrokeBeginPayload, StrokeCancelPayload,
    StrokeEndPayload, StrokeRemovePayload,
};

/// Records that a Stroke began with its initial point chunk.
pub const STROKE_BEGAN_EVENT_TYPE: &str = "canvas.stroke.began";
/// Records that a point chunk was appended to a Stroke.
pub const STROKE_POINTS_APPENDED_EVENT_TYPE: &str = "canvas.stroke.points_appended";
/// Records that a Stroke was completed.
pub const STROKE_COMPLETED_EVENT_TYPE: &str = "canvas.stroke.completed";
/// Records that an active Stroke was cancelled.
pub const STROKE_CANCELLED_EVENT_TYPE: &str = "canvas.stroke.cancelled";
/// Records that a completed Stroke was removed.
pub const STROKE_REMOVED_EVENT_TYPE: &str = "canvas.stroke.removed";

/// Stable classification of Canvas event facts.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum CanvasEventKind {
    /// A Stroke and its initial point chunk were created.
    StrokeBegan,
    /// A contiguous point chunk was appended.
    StrokePointsAppended,
    /// An active Stroke was completed.
    StrokeCompleted,
    /// An active Stroke was cancelled.
    StrokeCancelled,
    /// A completed Stroke was removed from the visible projection.
    StrokeRemoved,
}

impl CanvasEventKind {
    /// Classifies a protocol event type, returning `None` for non-Canvas events.
    #[must_use]
    pub fn from_event_type(event_type: &EventType) -> Option<Self> {
        match event_type.as_str() {
            STROKE_BEGAN_EVENT_TYPE => Some(Self::StrokeBegan),
            STROKE_POINTS_APPENDED_EVENT_TYPE => Some(Self::StrokePointsAppended),
            STROKE_COMPLETED_EVENT_TYPE => Some(Self::StrokeCompleted),
            STROKE_CANCELLED_EVENT_TYPE => Some(Self::StrokeCancelled),
            STROKE_REMOVED_EVENT_TYPE => Some(Self::StrokeRemoved),
            _ => None,
        }
    }
}

/// Reports whether an event type is recognized by Canvas Protocol v0.1.
#[must_use]
pub fn is_canvas_event_type(event_type: &EventType) -> bool {
    CanvasEventKind::from_event_type(event_type).is_some()
}

/// Strong payload representation of a recognized Canvas event.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum CanvasEventData {
    /// Payload of `canvas.stroke.began`.
    StrokeBegan(StrokeBeginPayload),
    /// Payload of `canvas.stroke.points_appended`.
    StrokePointsAppended(StrokeAppendPayload),
    /// Payload of `canvas.stroke.completed`.
    StrokeCompleted(StrokeEndPayload),
    /// Payload of `canvas.stroke.cancelled`.
    StrokeCancelled(StrokeCancelPayload),
    /// Payload of `canvas.stroke.removed`.
    StrokeRemoved(StrokeRemovePayload),
}

impl CanvasEventData {
    /// Returns this event's stable Canvas kind.
    #[must_use]
    pub const fn kind(&self) -> CanvasEventKind {
        match self {
            Self::StrokeBegan(_) => CanvasEventKind::StrokeBegan,
            Self::StrokePointsAppended(_) => CanvasEventKind::StrokePointsAppended,
            Self::StrokeCompleted(_) => CanvasEventKind::StrokeCompleted,
            Self::StrokeCancelled(_) => CanvasEventKind::StrokeCancelled,
            Self::StrokeRemoved(_) => CanvasEventKind::StrokeRemoved,
        }
    }
}

impl TryFrom<&Event> for CanvasEventData {
    type Error = CanvasProjectionError;

    fn try_from(event: &Event) -> Result<Self, Self::Error> {
        let kind = CanvasEventKind::from_event_type(event.event_type()).ok_or_else(|| {
            CanvasProjectionError::UnexpectedEvent {
                event_type: event.event_type().clone(),
            }
        })?;
        let invalid_payload = |source| CanvasProjectionError::InvalidEventPayload {
            event_type: event.event_type().clone(),
            source,
        };

        match kind {
            CanvasEventKind::StrokeBegan => StrokeBeginPayload::try_from(event.payload())
                .map(Self::StrokeBegan)
                .map_err(invalid_payload),
            CanvasEventKind::StrokePointsAppended => StrokeAppendPayload::try_from(event.payload())
                .map(Self::StrokePointsAppended)
                .map_err(invalid_payload),
            CanvasEventKind::StrokeCompleted => StrokeEndPayload::try_from(event.payload())
                .map(Self::StrokeCompleted)
                .map_err(invalid_payload),
            CanvasEventKind::StrokeCancelled => StrokeCancelPayload::try_from(event.payload())
                .map(Self::StrokeCancelled)
                .map_err(invalid_payload),
            CanvasEventKind::StrokeRemoved => StrokeRemovePayload::try_from(event.payload())
                .map(Self::StrokeRemoved)
                .map_err(invalid_payload),
        }
    }
}

#[cfg(test)]
mod tests {
    use orbitrelay_core::{Metadata, Timestamp};
    use orbitrelay_protocol::{ActionId, ActorId, Event, EventId, EventType, Payload, SessionId};

    use super::{
        is_canvas_event_type, CanvasEventData, CanvasEventKind, STROKE_BEGAN_EVENT_TYPE,
        STROKE_CANCELLED_EVENT_TYPE, STROKE_COMPLETED_EVENT_TYPE,
        STROKE_POINTS_APPENDED_EVENT_TYPE, STROKE_REMOVED_EVENT_TYPE,
    };
    use crate::CanvasProjectionError;

    #[test]
    fn event_type_names_are_stable() {
        assert_eq!(STROKE_BEGAN_EVENT_TYPE, "canvas.stroke.began");
        assert_eq!(
            STROKE_POINTS_APPENDED_EVENT_TYPE,
            "canvas.stroke.points_appended"
        );
        assert_eq!(STROKE_COMPLETED_EVENT_TYPE, "canvas.stroke.completed");
        assert_eq!(STROKE_CANCELLED_EVENT_TYPE, "canvas.stroke.cancelled");
        assert_eq!(STROKE_REMOVED_EVENT_TYPE, "canvas.stroke.removed");
    }

    #[test]
    fn classifies_canvas_and_non_canvas_event_types() {
        assert_eq!(
            CanvasEventKind::from_event_type(&EventType::new(STROKE_BEGAN_EVENT_TYPE)),
            Some(CanvasEventKind::StrokeBegan)
        );
        assert!(is_canvas_event_type(&EventType::new(
            STROKE_POINTS_APPENDED_EVENT_TYPE
        )));
        assert!(!is_canvas_event_type(&EventType::new("document.updated")));
    }

    #[test]
    fn recognized_event_with_invalid_payload_fails_strictly() {
        let event = Event::new(
            EventId::new(),
            SessionId::new(),
            ActorId::new(),
            ActionId::new(),
            EventType::new(STROKE_BEGAN_EVENT_TYPE),
            Timestamp::from_unix_timestamp(1_700_000_000).expect("timestamp should be valid"),
            Payload::new(),
            Metadata::new(),
        );

        assert!(matches!(
            CanvasEventData::try_from(&event),
            Err(CanvasProjectionError::InvalidEventPayload { .. })
        ));
    }

    #[test]
    fn non_canvas_event_is_not_decoded_as_canvas_data() {
        let event = Event::new(
            EventId::new(),
            SessionId::new(),
            ActorId::new(),
            ActionId::new(),
            EventType::new("document.updated"),
            Timestamp::from_unix_timestamp(1_700_000_000).expect("timestamp should be valid"),
            Payload::new(),
            Metadata::new(),
        );

        assert!(matches!(
            CanvasEventData::try_from(&event),
            Err(CanvasProjectionError::UnexpectedEvent { .. })
        ));
    }
}
