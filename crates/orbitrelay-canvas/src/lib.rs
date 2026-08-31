//! Pure Canvas and whiteboard domain protocol for OrbitRelay.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod action;
mod descriptor;
mod error;
mod event;
mod geometry;
mod id;
mod lifecycle;
mod payload;
mod projection;
mod style;

pub use action::{
    STROKE_APPEND_ACTION_TYPE, STROKE_BEGIN_ACTION_TYPE, STROKE_CANCEL_ACTION_TYPE,
    STROKE_END_ACTION_TYPE, STROKE_REMOVE_ACTION_TYPE,
};
pub use descriptor::CanvasDescriptor;
pub use error::{CanvasError, CanvasProjectionError};
pub use event::{
    is_canvas_event_type, CanvasEventData, CanvasEventKind, STROKE_BEGAN_EVENT_TYPE,
    STROKE_CANCELLED_EVENT_TYPE, STROKE_COMPLETED_EVENT_TYPE, STROKE_POINTS_APPENDED_EVENT_TYPE,
    STROKE_REMOVED_EVENT_TYPE,
};
pub use geometry::{CanvasPoint, CanvasSpace};
pub use id::{CanvasId, LayerId, StrokeId};
pub use lifecycle::StrokeLifecycle;
pub use payload::{
    validate_append_chunk_index, validate_begin_chunk_index, StrokeAppendPayload,
    StrokeBeginPayload, StrokeCancelPayload, StrokeEndPayload, StrokeRemovePayload,
    MAX_POINTS_PER_CHUNK,
};
pub use projection::{StrokeChunk, StrokeProjection, StrokeProjector};
pub use style::{RgbaColor, StrokeStyle, StrokeTool};
