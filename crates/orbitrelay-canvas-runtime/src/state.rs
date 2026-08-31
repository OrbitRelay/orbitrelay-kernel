//! Read-only current Stroke state boundary.

use async_trait::async_trait;
use orbitrelay_canvas::{CanvasId, StrokeId, StrokeProjection};
use orbitrelay_protocol::SessionId;

use crate::CanvasStateReadError;

/// Reads Stroke state derived from already-persisted Canvas events.
///
/// This port is intentionally read-only. Implementations may replay events or
/// use a rebuildable projection, but they must not treat this API as a second
/// fact store.
#[async_trait]
pub trait CanvasStateReader: Send + Sync {
    /// Loads one Stroke projection in its expected Session and Canvas context.
    async fn load_stroke(
        &self,
        session_id: &SessionId,
        canvas_id: &CanvasId,
        stroke_id: &StrokeId,
    ) -> Result<Option<StrokeProjection>, CanvasStateReadError>;
}
