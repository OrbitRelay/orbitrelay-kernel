//! Runtime handlers and read ports for the OrbitRelay Canvas domain.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod catalog;
mod error;
mod handlers;
mod service;
mod state;

pub use catalog::CanvasCatalog;
pub use error::{CanvasCatalogError, CanvasRuntimeError, CanvasStateReadError};
pub use handlers::{
    register_canvas_handlers, StrokeAppendHandler, StrokeBeginHandler, StrokeCancelHandler,
    StrokeEndHandler, StrokeRemoveHandler, CANVAS_STROKE_EXECUTION_NAMESPACE,
};
pub use service::CanvasCommandService;
pub use state::CanvasStateReader;
