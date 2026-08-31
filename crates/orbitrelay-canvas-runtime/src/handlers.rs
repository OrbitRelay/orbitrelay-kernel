//! Runtime handler adapters for the five Canvas Stroke actions.

use std::sync::Arc;

use async_trait::async_trait;
use orbitrelay_canvas::{
    CanvasError, StrokeAppendPayload, StrokeBeginPayload, StrokeCancelPayload, StrokeEndPayload,
    StrokeId, StrokeRemovePayload, STROKE_APPEND_ACTION_TYPE, STROKE_BEGIN_ACTION_TYPE,
    STROKE_CANCEL_ACTION_TYPE, STROKE_END_ACTION_TYPE, STROKE_REMOVE_ACTION_TYPE,
};
use orbitrelay_protocol::{Action, ActionType, Payload};
use orbitrelay_runtime::{
    ActionHandler, EventDraft, ExecutionScope, HandlerError, HandlerRegistry, RegistryError,
    RuntimeContext,
};

use crate::CanvasCommandService;

/// Stable execution-coordination namespace for all Stroke aggregate commands.
pub const CANVAS_STROKE_EXECUTION_NAMESPACE: &str = "canvas.stroke";

macro_rules! define_handler {
    ($(#[$attribute:meta])* $name:ident, $payload:ty, $method:ident) => {
        $(#[$attribute])*
        pub struct $name {
            service: Arc<CanvasCommandService>,
        }

        impl $name {
            /// Creates a handler backed by a shared Canvas command service.
            #[must_use]
            pub fn new(service: Arc<CanvasCommandService>) -> Self {
                Self { service }
            }
        }

        #[async_trait]
        impl ActionHandler for $name {
            async fn validate(
                &self,
                action: &Action,
                _context: &RuntimeContext,
            ) -> Result<(), HandlerError> {
                decode_payload::<$payload>(action).map(|_| ())
            }

            fn execution_scope(
                &self,
                action: &Action,
            ) -> Result<Option<ExecutionScope>, HandlerError> {
                let payload = decode_payload::<$payload>(action)?;
                stroke_scope(payload.stroke_id()).map(Some)
            }

            async fn handle(
                &self,
                action: &Action,
                _context: &RuntimeContext,
            ) -> Result<Vec<EventDraft>, HandlerError> {
                let payload = decode_payload::<$payload>(action)?;
                self.service
                    .$method(action, &payload)
                    .await
                    .map_err(HandlerError::from)
            }
        }
    };
}

define_handler!(
    /// Handles `canvas.stroke.begin` actions.
    StrokeBeginHandler,
    StrokeBeginPayload,
    begin
);
define_handler!(
    /// Handles `canvas.stroke.append` actions.
    StrokeAppendHandler,
    StrokeAppendPayload,
    append
);
define_handler!(
    /// Handles `canvas.stroke.end` actions.
    StrokeEndHandler,
    StrokeEndPayload,
    end
);
define_handler!(
    /// Handles `canvas.stroke.cancel` actions.
    StrokeCancelHandler,
    StrokeCancelPayload,
    cancel
);
define_handler!(
    /// Handles `canvas.stroke.remove` actions.
    StrokeRemoveHandler,
    StrokeRemovePayload,
    remove
);

/// Registers exactly the five Canvas Stroke handlers in an existing registry.
pub fn register_canvas_handlers(
    registry: &HandlerRegistry,
    service: Arc<CanvasCommandService>,
) -> Result<(), RegistryError> {
    registry.register(
        ActionType::new(STROKE_BEGIN_ACTION_TYPE),
        Arc::new(StrokeBeginHandler::new(service.clone())),
    )?;
    registry.register(
        ActionType::new(STROKE_APPEND_ACTION_TYPE),
        Arc::new(StrokeAppendHandler::new(service.clone())),
    )?;
    registry.register(
        ActionType::new(STROKE_END_ACTION_TYPE),
        Arc::new(StrokeEndHandler::new(service.clone())),
    )?;
    registry.register(
        ActionType::new(STROKE_CANCEL_ACTION_TYPE),
        Arc::new(StrokeCancelHandler::new(service.clone())),
    )?;
    registry.register(
        ActionType::new(STROKE_REMOVE_ACTION_TYPE),
        Arc::new(StrokeRemoveHandler::new(service)),
    )?;
    Ok(())
}

fn decode_payload<T>(action: &Action) -> Result<T, HandlerError>
where
    for<'a> T: TryFrom<&'a Payload, Error = CanvasError>,
{
    T::try_from(action.payload()).map_err(|_| HandlerError::new("invalid Canvas action payload"))
}

fn stroke_scope(stroke_id: &StrokeId) -> Result<ExecutionScope, HandlerError> {
    ExecutionScope::new(CANVAS_STROKE_EXECUTION_NAMESPACE, stroke_id.to_string())
        .map_err(|_| HandlerError::new("invalid Canvas execution scope"))
}
