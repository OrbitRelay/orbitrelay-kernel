#![allow(dead_code)]

use std::sync::Arc;

use async_trait::async_trait;
use orbitrelay_canvas::{
    CanvasDescriptor, CanvasError, CanvasId, CanvasPoint, CanvasProjectionError, CanvasSpace,
    LayerId, RgbaColor, StrokeAppendPayload, StrokeBeginPayload, StrokeCancelPayload,
    StrokeEndPayload, StrokeId, StrokeLifecycle, StrokeProjection, StrokeProjector,
    StrokeRemovePayload, StrokeStyle, StrokeTool, STROKE_BEGAN_EVENT_TYPE,
    STROKE_CANCELLED_EVENT_TYPE, STROKE_COMPLETED_EVENT_TYPE, STROKE_POINTS_APPENDED_EVENT_TYPE,
    STROKE_REMOVED_EVENT_TYPE,
};
use orbitrelay_canvas_runtime::{
    CanvasCatalog, CanvasCatalogError, CanvasCommandService, CanvasStateReadError,
    CanvasStateReader,
};
use orbitrelay_core::{Metadata, Timestamp};
use orbitrelay_protocol::{
    Action, ActionId, ActionType, ActorId, Event, EventId, EventType, Payload, SessionId,
};

pub struct Fixture {
    pub session_id: SessionId,
    pub actor_id: ActorId,
    pub canvas_id: CanvasId,
    pub layer_id: LayerId,
    pub stroke_id: StrokeId,
}

impl Fixture {
    pub fn new() -> Self {
        Self {
            session_id: SessionId::new(),
            actor_id: ActorId::new(),
            canvas_id: CanvasId::new(),
            layer_id: LayerId::new(),
            stroke_id: StrokeId::new(),
        }
    }

    pub fn descriptor(&self) -> CanvasDescriptor {
        CanvasDescriptor::new(
            self.canvas_id.clone(),
            self.session_id.clone(),
            CanvasSpace::new(100.0, 100.0).expect("fixture space should be valid"),
            [self.layer_id.clone()],
            self.layer_id.clone(),
        )
        .expect("fixture descriptor should be valid")
    }

    pub fn begin_payload(&self) -> StrokeBeginPayload {
        self.begin_payload_with(self.actor_id.clone(), style(2.0), point(1.0))
            .0
    }

    pub fn begin_payload_with(
        &self,
        actor_id: ActorId,
        style: StrokeStyle,
        point: CanvasPoint,
    ) -> (StrokeBeginPayload, ActorId) {
        (
            StrokeBeginPayload::new(
                self.canvas_id.clone(),
                self.layer_id.clone(),
                self.stroke_id.clone(),
                StrokeTool::Pen,
                style,
                0,
                [point],
            )
            .expect("fixture begin payload should be valid"),
            actor_id,
        )
    }

    pub fn action<T>(&self, action_type: &str, payload: &T) -> Action
    where
        for<'a> Payload: TryFrom<&'a T, Error = CanvasError>,
    {
        self.action_as(self.actor_id.clone(), action_type, payload)
    }

    pub fn action_as<T>(&self, actor_id: ActorId, action_type: &str, payload: &T) -> Action
    where
        for<'a> Payload: TryFrom<&'a T, Error = CanvasError>,
    {
        Action::new(
            ActionId::new(),
            self.session_id.clone(),
            actor_id,
            ActionType::new(action_type),
            Timestamp::from_unix_timestamp(1_700_000_000).expect("timestamp should be valid"),
            Payload::try_from(payload).expect("fixture payload should encode"),
            Metadata::new(),
        )
    }

    pub fn projection(&self, lifecycle: StrokeLifecycle, with_append: bool) -> StrokeProjection {
        let begin = self.begin_payload();
        let began = self.event(STROKE_BEGAN_EVENT_TYPE, &begin);
        let mut projection =
            StrokeProjector::apply(None, &began).expect("fixture begin should project");

        if with_append {
            let append = StrokeAppendPayload::new(
                self.canvas_id.clone(),
                self.stroke_id.clone(),
                1,
                [point(2.0)],
            )
            .expect("fixture append should be valid");
            projection = StrokeProjector::apply(
                Some(projection),
                &self.event(STROKE_POINTS_APPENDED_EVENT_TYPE, &append),
            )
            .expect("fixture append should project");
        }

        match lifecycle {
            StrokeLifecycle::Active => projection,
            StrokeLifecycle::Completed => {
                let payload = StrokeEndPayload::new(
                    self.canvas_id.clone(),
                    self.stroke_id.clone(),
                    projection.last_chunk_index(),
                );
                StrokeProjector::apply(
                    Some(projection),
                    &self.event(STROKE_COMPLETED_EVENT_TYPE, &payload),
                )
                .expect("fixture completion should project")
            }
            StrokeLifecycle::Cancelled => {
                let payload = StrokeCancelPayload::new(
                    self.canvas_id.clone(),
                    self.stroke_id.clone(),
                    projection.last_chunk_index(),
                );
                StrokeProjector::apply(
                    Some(projection),
                    &self.event(STROKE_CANCELLED_EVENT_TYPE, &payload),
                )
                .expect("fixture cancellation should project")
            }
            StrokeLifecycle::Removed => {
                let completed = self.projection(StrokeLifecycle::Completed, with_append);
                let payload =
                    StrokeRemovePayload::new(self.canvas_id.clone(), self.stroke_id.clone());
                StrokeProjector::apply(
                    Some(completed),
                    &self.event(STROKE_REMOVED_EVENT_TYPE, &payload),
                )
                .expect("fixture removal should project")
            }
            _ => panic!("fixture does not support future lifecycle variants"),
        }
    }

    fn event<T>(&self, event_type: &str, payload: &T) -> Event
    where
        for<'a> Payload: TryFrom<&'a T, Error = CanvasError>,
    {
        Event::new(
            EventId::new(),
            self.session_id.clone(),
            self.actor_id.clone(),
            ActionId::new(),
            EventType::new(event_type),
            Timestamp::from_unix_timestamp(1_700_000_000).expect("timestamp should be valid"),
            Payload::try_from(payload).expect("fixture payload should encode"),
            Metadata::new(),
        )
    }
}

pub fn point(value: f64) -> CanvasPoint {
    CanvasPoint::new(value, value).expect("fixture point should be finite")
}

pub fn style(width: f64) -> StrokeStyle {
    StrokeStyle::new(width, RgbaColor::new(10, 20, 30, 255)).expect("fixture style should be valid")
}

pub enum CatalogResponse {
    Found(CanvasDescriptor),
    Missing,
    Failed,
}

pub struct TestCanvasCatalog {
    response: CatalogResponse,
}

impl TestCanvasCatalog {
    pub fn found(descriptor: CanvasDescriptor) -> Self {
        Self {
            response: CatalogResponse::Found(descriptor),
        }
    }

    pub fn missing() -> Self {
        Self {
            response: CatalogResponse::Missing,
        }
    }

    pub fn failed() -> Self {
        Self {
            response: CatalogResponse::Failed,
        }
    }
}

#[async_trait]
impl CanvasCatalog for TestCanvasCatalog {
    async fn get_canvas(
        &self,
        _canvas_id: &CanvasId,
    ) -> Result<Option<CanvasDescriptor>, CanvasCatalogError> {
        match &self.response {
            CatalogResponse::Found(descriptor) => Ok(Some(descriptor.clone())),
            CatalogResponse::Missing => Ok(None),
            CatalogResponse::Failed => Err(CanvasCatalogError::new("test catalog failure")),
        }
    }
}

pub enum StateResponse {
    Found(StrokeProjection),
    Missing,
    Failed,
    Corrupted(CanvasProjectionError),
}

pub struct MockCanvasStateReader {
    response: StateResponse,
}

impl MockCanvasStateReader {
    pub fn found(projection: StrokeProjection) -> Self {
        Self {
            response: StateResponse::Found(projection),
        }
    }

    pub fn missing() -> Self {
        Self {
            response: StateResponse::Missing,
        }
    }

    pub fn failed() -> Self {
        Self {
            response: StateResponse::Failed,
        }
    }

    pub fn corrupted(error: CanvasProjectionError) -> Self {
        Self {
            response: StateResponse::Corrupted(error),
        }
    }
}

#[async_trait]
impl CanvasStateReader for MockCanvasStateReader {
    async fn load_stroke(
        &self,
        _session_id: &SessionId,
        _canvas_id: &CanvasId,
        _stroke_id: &StrokeId,
    ) -> Result<Option<StrokeProjection>, CanvasStateReadError> {
        match &self.response {
            StateResponse::Found(projection) => Ok(Some(projection.clone())),
            StateResponse::Missing => Ok(None),
            StateResponse::Failed => Err(CanvasStateReadError::unavailable("test state failure")),
            StateResponse::Corrupted(error) => {
                Err(CanvasStateReadError::projection_corrupted(error.clone()))
            }
        }
    }
}

pub fn service(fixture: &Fixture, state_reader: MockCanvasStateReader) -> CanvasCommandService {
    CanvasCommandService::new(
        Arc::new(TestCanvasCatalog::found(fixture.descriptor())),
        Arc::new(state_reader),
    )
}

pub fn service_with(
    catalog: TestCanvasCatalog,
    state_reader: MockCanvasStateReader,
) -> CanvasCommandService {
    CanvasCommandService::new(Arc::new(catalog), Arc::new(state_reader))
}
