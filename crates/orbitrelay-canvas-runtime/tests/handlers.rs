mod common;

use std::sync::Arc;

use async_trait::async_trait;
use orbitrelay_canvas::{
    StrokeAppendPayload, StrokeCancelPayload, StrokeEndPayload, StrokeId, StrokeRemovePayload,
    STROKE_APPEND_ACTION_TYPE, STROKE_BEGIN_ACTION_TYPE, STROKE_CANCEL_ACTION_TYPE,
    STROKE_END_ACTION_TYPE, STROKE_REMOVE_ACTION_TYPE,
};
use orbitrelay_canvas_runtime::{
    register_canvas_handlers, StrokeAppendHandler, StrokeBeginHandler, StrokeCancelHandler,
    StrokeEndHandler, StrokeRemoveHandler, CANVAS_STROKE_EXECUTION_NAMESPACE,
};
use orbitrelay_core::{Metadata, Timestamp};
use orbitrelay_protocol::{Action, ActionId, ActionType, Payload};
use orbitrelay_runtime::{
    ActionAuthorizer, ActionHandler, AuthorizationError, Clock, HandlerRegistry, RuntimeContext,
};

use common::{point, service, service_with, Fixture, MockCanvasStateReader, TestCanvasCatalog};

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        Timestamp::from_unix_timestamp(1_700_000_000).expect("timestamp should be valid")
    }
}

struct AllowAuthorizer;

#[async_trait]
impl ActionAuthorizer for AllowAuthorizer {
    async fn authorize(&self, _action: &Action) -> Result<(), AuthorizationError> {
        Ok(())
    }
}

fn context() -> RuntimeContext {
    RuntimeContext::new(Arc::new(FixedClock), Arc::new(AllowAuthorizer))
}

#[test]
fn all_handlers_map_one_stroke_to_the_same_scope() {
    let fixture = Fixture::new();
    let service = Arc::new(service(&fixture, MockCanvasStateReader::missing()));
    let begin = fixture.begin_payload();
    let append = StrokeAppendPayload::new(
        fixture.canvas_id.clone(),
        fixture.stroke_id.clone(),
        1,
        [point(2.0)],
    )
    .expect("append should be valid");
    let end = StrokeEndPayload::new(fixture.canvas_id.clone(), fixture.stroke_id.clone(), 0);
    let cancel = StrokeCancelPayload::new(fixture.canvas_id.clone(), fixture.stroke_id.clone(), 0);
    let remove = StrokeRemovePayload::new(fixture.canvas_id.clone(), fixture.stroke_id.clone());

    let cases: Vec<(Box<dyn ActionHandler>, Action)> = vec![
        (
            Box::new(StrokeBeginHandler::new(service.clone())),
            fixture.action(STROKE_BEGIN_ACTION_TYPE, &begin),
        ),
        (
            Box::new(StrokeAppendHandler::new(service.clone())),
            fixture.action(STROKE_APPEND_ACTION_TYPE, &append),
        ),
        (
            Box::new(StrokeEndHandler::new(service.clone())),
            fixture.action(STROKE_END_ACTION_TYPE, &end),
        ),
        (
            Box::new(StrokeCancelHandler::new(service.clone())),
            fixture.action(STROKE_CANCEL_ACTION_TYPE, &cancel),
        ),
        (
            Box::new(StrokeRemoveHandler::new(service)),
            fixture.action(STROKE_REMOVE_ACTION_TYPE, &remove),
        ),
    ];

    for (handler, action) in cases {
        let scope = handler
            .execution_scope(&action)
            .expect("scope resolution should succeed")
            .expect("Canvas handler should be scoped");
        assert_eq!(scope.namespace(), CANVAS_STROKE_EXECUTION_NAMESPACE);
        assert_eq!(scope.key(), fixture.stroke_id.to_string());
    }
}

#[test]
fn different_strokes_map_to_different_scope_keys() {
    let first = Fixture::new();
    let second = Fixture::new();
    let service = Arc::new(service(&first, MockCanvasStateReader::missing()));
    let handler = StrokeBeginHandler::new(service);

    let first_scope = handler
        .execution_scope(&first.action(STROKE_BEGIN_ACTION_TYPE, &first.begin_payload()))
        .expect("scope should resolve")
        .expect("scope should exist");
    let second_payload = orbitrelay_canvas::StrokeBeginPayload::new(
        first.canvas_id.clone(),
        first.layer_id.clone(),
        second.stroke_id.clone(),
        orbitrelay_canvas::StrokeTool::Pen,
        common::style(2.0),
        0,
        [point(1.0)],
    )
    .expect("payload should be valid");
    let second_scope = handler
        .execution_scope(&first.action(STROKE_BEGIN_ACTION_TYPE, &second_payload))
        .expect("scope should resolve")
        .expect("scope should exist");

    assert_ne!(first_scope, second_scope);
    assert_eq!(second_scope.key(), second.stroke_id.to_string());
}

#[tokio::test]
async fn validate_rejects_invalid_payload_without_using_runtime_ports() {
    let fixture = Fixture::new();
    let service = Arc::new(service_with(
        TestCanvasCatalog::failed(),
        MockCanvasStateReader::failed(),
    ));
    let handler = StrokeBeginHandler::new(service);
    let invalid = Action::new(
        ActionId::new(),
        fixture.session_id.clone(),
        fixture.actor_id.clone(),
        ActionType::new(STROKE_BEGIN_ACTION_TYPE),
        Timestamp::from_unix_timestamp(1_700_000_000).expect("timestamp should be valid"),
        Payload::new(),
        Metadata::new(),
    );

    let error = handler
        .validate(&invalid, &context())
        .await
        .expect_err("empty payload should be rejected");
    assert_eq!(error.message(), "invalid Canvas action payload");
}

#[tokio::test]
async fn handler_produces_draft_and_sanitizes_backend_failure() {
    let fixture = Fixture::new();
    let payload = fixture.begin_payload();
    let action = fixture.action(STROKE_BEGIN_ACTION_TYPE, &payload);
    let handler = StrokeBeginHandler::new(Arc::new(service(
        &fixture,
        MockCanvasStateReader::missing(),
    )));

    handler
        .validate(&action, &context())
        .await
        .expect("payload should validate");
    let drafts = handler
        .handle(&action, &context())
        .await
        .expect("new Stroke should produce a draft");
    assert_eq!(drafts.len(), 1);

    let failed_handler = StrokeBeginHandler::new(Arc::new(service_with(
        TestCanvasCatalog::failed(),
        MockCanvasStateReader::missing(),
    )));
    let error = failed_handler
        .handle(&action, &context())
        .await
        .expect_err("catalog failure should reach HandlerError");
    assert_eq!(error.message(), "Canvas catalog is unavailable");
    assert!(!error.message().contains("test catalog failure"));
}

#[test]
fn registration_helper_registers_exactly_five_canvas_action_types() {
    let fixture = Fixture::new();
    let registry = HandlerRegistry::new();
    register_canvas_handlers(
        &registry,
        Arc::new(service(&fixture, MockCanvasStateReader::missing())),
    )
    .expect("Canvas handlers should register");

    for action_type in [
        STROKE_BEGIN_ACTION_TYPE,
        STROKE_APPEND_ACTION_TYPE,
        STROKE_END_ACTION_TYPE,
        STROKE_CANCEL_ACTION_TYPE,
        STROKE_REMOVE_ACTION_TYPE,
    ] {
        assert!(registry.contains(&ActionType::new(action_type)));
    }
    assert!(!registry.contains(&ActionType::new("canvas.layer.create")));
}

#[test]
fn stroke_scope_key_is_the_canonical_stroke_identifier() {
    let stroke_id = StrokeId::new();
    assert_eq!(
        stroke_id.to_string().parse::<StrokeId>().unwrap(),
        stroke_id
    );
}
