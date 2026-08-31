//! Trusted Canvas metadata lookup boundary.

use async_trait::async_trait;
use orbitrelay_canvas::{CanvasDescriptor, CanvasId};

use crate::CanvasCatalogError;

/// Reads trusted Canvas descriptors without prescribing their storage.
#[async_trait]
pub trait CanvasCatalog: Send + Sync {
    /// Returns the requested Canvas descriptor or `None` when it does not exist.
    async fn get_canvas(
        &self,
        canvas_id: &CanvasId,
    ) -> Result<Option<CanvasDescriptor>, CanvasCatalogError>;
}
