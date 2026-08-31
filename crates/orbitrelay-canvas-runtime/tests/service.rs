mod common;

use orbitrelay_canvas::{
    CanvasError, CanvasPoint, CanvasProjectionError, LayerId, StrokeAppendPayload,
    StrokeBeginPayload, StrokeCancelPayload, StrokeEndPayload, StrokeLifecycle,
    StrokeRemovePayload, StrokeTool, STROKE_BEGAN_EVENT_TYPE, STROKE_CANCELLED_EVENT_TYPE,
    STROKE_COMPLETED_EVENT_TYPE, STROKE_POINTS_APPENDED_EVENT_TYPE, STROKE_REMOVED_EVENT_TYPE,
};
use orbitrelay_canvas_runtime::CanvasRuntimeError;
use orbitrelay_protocol::ActorId;

use common::{
    point, service, service_with, style, Fixture, MockCanvasStateReader, TestCanvasCatalog,
};

#[tokio::test]
async fn begin_creates_one_fact_for_a_new_stroke() {
    let fixture = Fixture::new();
    let payload = fixture.begin_payload();
    let action = fixture.action("canvas.stroke.begin", &payload);
    let drafts = service(&fixture, MockCanvasStateReader::missing())
        .begin(&action, &payload)
        .await
        .expect("new Stroke should begin");

    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0].event_type().as_str(), STROKE_BEGAN_EVENT_TYPE);
    assert_eq!(
        StrokeBeginPayload::try_from(drafts[0].payload()).expect("draft should decode"),
        payload
    );
    assert!(drafts[0].metadata().is_empty());
}

#[tokio::test]
async fn begin_distinguishes_catalog_absence_failure_session_layer_and_bounds() {
    let fixture = Fixture::new();
    let payload = fixture.begin_payload();
    let action = fixture.action("canvas.stroke.begin", &payload);

    let missing = service_with(
        TestCanvasCatalog::missing(),
        MockCanvasStateReader::missing(),
    )
    .begin(&action, &payload)
    .await
    .expect_err("missing Canvas should fail");
    assert!(matches!(missing, CanvasRuntimeError::CanvasNotFound { .. }));

    let failed = service_with(
        TestCanvasCatalog::failed(),
        MockCanvasStateReader::missing(),
    )
    .begin(&action, &payload)
    .await
    .expect_err("catalog failure should remain distinct");
    assert!(matches!(failed, CanvasRuntimeError::CatalogFailed { .. }));

    let foreign_action = orbitrelay_protocol::Action::new(
        action.id().clone(),
        orbitrelay_protocol::SessionId::new(),
        action.actor_id().clone(),
        action.action_type().clone(),
        action.requested_at().clone(),
        action.payload().clone(),
        action.metadata().clone(),
    );
    let mismatch = service(&fixture, MockCanvasStateReader::missing())
        .begin(&foreign_action, &payload)
        .await
        .expect_err("foreign Session should fail");
    assert!(matches!(
        mismatch,
        CanvasRuntimeError::CanvasSessionMismatch { .. }
    ));

    let foreign_layer_payload = StrokeBeginPayload::new(
        fixture.canvas_id.clone(),
        LayerId::new(),
        fixture.stroke_id.clone(),
        StrokeTool::Pen,
        style(2.0),
        0,
        [point(1.0)],
    )
    .expect("payload should be structurally valid");
    let layer_error = service(&fixture, MockCanvasStateReader::missing())
        .begin(&action, &foreign_layer_payload)
        .await
        .expect_err("foreign layer should fail");
    assert!(matches!(
        layer_error,
        CanvasRuntimeError::LayerNotFound { .. }
    ));

    let outside_payload = StrokeBeginPayload::new(
        fixture.canvas_id.clone(),
        fixture.layer_id.clone(),
        fixture.stroke_id.clone(),
        StrokeTool::Pen,
        style(2.0),
        0,
        [CanvasPoint::new(101.0, 10.0).expect("point should be finite")],
    )
    .expect("payload should be structurally valid");
    let bounds_error = service(&fixture, MockCanvasStateReader::missing())
        .begin(&action, &outside_payload)
        .await
        .expect_err("out-of-bounds point should fail");
    assert!(matches!(
        bounds_error,
        CanvasRuntimeError::DomainViolation {
            source: CanvasError::InvalidCoordinate { .. }
        }
    ));
}

#[tokio::test]
async fn identical_begin_retry_is_a_no_op() {
    let fixture = Fixture::new();
    let payload = fixture.begin_payload();
    let action = fixture.action("canvas.stroke.begin", &payload);
    let drafts = service(
        &fixture,
        MockCanvasStateReader::found(fixture.projection(StrokeLifecycle::Completed, true)),
    )
    .begin(&action, &payload)
    .await
    .expect("identical historical begin should be idempotent");

    assert!(drafts.is_empty());
}

#[tokio::test]
async fn begin_retry_rejects_different_points_style_or_creator() {
    let fixture = Fixture::new();
    let projection = fixture.projection(StrokeLifecycle::Active, false);

    let different_points = StrokeBeginPayload::new(
        fixture.canvas_id.clone(),
        fixture.layer_id.clone(),
        fixture.stroke_id.clone(),
        StrokeTool::Pen,
        style(2.0),
        0,
        [point(3.0)],
    )
    .expect("payload should be valid");
    let error = service(&fixture, MockCanvasStateReader::found(projection.clone()))
        .begin(
            &fixture.action("canvas.stroke.begin", &different_points),
            &different_points,
        )
        .await
        .expect_err("different points should conflict");
    assert_stroke_already_exists(error);

    let different_style = StrokeBeginPayload::new(
        fixture.canvas_id.clone(),
        fixture.layer_id.clone(),
        fixture.stroke_id.clone(),
        StrokeTool::Pen,
        style(3.0),
        0,
        [point(1.0)],
    )
    .expect("payload should be valid");
    let error = service(&fixture, MockCanvasStateReader::found(projection.clone()))
        .begin(
            &fixture.action("canvas.stroke.begin", &different_style),
            &different_style,
        )
        .await
        .expect_err("different style should conflict");
    assert_stroke_already_exists(error);

    let payload = fixture.begin_payload();
    let error = service(&fixture, MockCanvasStateReader::found(projection))
        .begin(
            &fixture.action_as(ActorId::new(), "canvas.stroke.begin", &payload),
            &payload,
        )
        .await
        .expect_err("different creator should conflict");
    assert_stroke_already_exists(error);
}

#[tokio::test]
async fn append_creates_only_the_next_chunk() {
    let fixture = Fixture::new();
    let payload = StrokeAppendPayload::new(
        fixture.canvas_id.clone(),
        fixture.stroke_id.clone(),
        1,
        [point(2.0)],
    )
    .expect("append should be valid");
    let action = fixture.action("canvas.stroke.append", &payload);
    let drafts = service(
        &fixture,
        MockCanvasStateReader::found(fixture.projection(StrokeLifecycle::Active, false)),
    )
    .append(&action, &payload)
    .await
    .expect("next chunk should append");

    assert_eq!(drafts.len(), 1);
    assert_eq!(
        drafts[0].event_type().as_str(),
        STROKE_POINTS_APPENDED_EVENT_TYPE
    );
    assert_eq!(
        StrokeAppendPayload::try_from(drafts[0].payload()).expect("draft should decode"),
        payload
    );
}

#[tokio::test]
async fn append_distinguishes_missing_stroke_missing_chunk_and_conflict() {
    let fixture = Fixture::new();
    let next = StrokeAppendPayload::new(
        fixture.canvas_id.clone(),
        fixture.stroke_id.clone(),
        1,
        [point(2.0)],
    )
    .expect("append should be valid");
    let action = fixture.action("canvas.stroke.append", &next);
    let missing_stroke = service(&fixture, MockCanvasStateReader::missing())
        .append(&action, &next)
        .await
        .expect_err("missing Stroke should fail");
    assert!(matches!(
        missing_stroke,
        CanvasRuntimeError::DomainViolation {
            source: CanvasError::StrokeNotFound { .. }
        }
    ));

    let gap = StrokeAppendPayload::new(
        fixture.canvas_id.clone(),
        fixture.stroke_id.clone(),
        3,
        [point(3.0)],
    )
    .expect("append should be valid");
    let gap_error = service(
        &fixture,
        MockCanvasStateReader::found(fixture.projection(StrokeLifecycle::Active, false)),
    )
    .append(&fixture.action("canvas.stroke.append", &gap), &gap)
    .await
    .expect_err("gap should fail");
    assert!(matches!(
        gap_error,
        CanvasRuntimeError::DomainViolation {
            source: CanvasError::MissingChunk {
                expected: 1,
                actual: 3,
                ..
            }
        }
    ));

    let conflict = StrokeAppendPayload::new(
        fixture.canvas_id.clone(),
        fixture.stroke_id.clone(),
        1,
        [point(9.0)],
    )
    .expect("append should be valid");
    let conflict_error = service(
        &fixture,
        MockCanvasStateReader::found(fixture.projection(StrokeLifecycle::Active, true)),
    )
    .append(
        &fixture.action("canvas.stroke.append", &conflict),
        &conflict,
    )
    .await
    .expect_err("occupied chunk with different content should conflict");
    assert!(matches!(
        conflict_error,
        CanvasRuntimeError::DomainViolation {
            source: CanvasError::ChunkConflict { chunk_index: 1, .. }
        }
    ));
}

#[tokio::test]
async fn old_identical_append_is_a_no_op_after_completion_or_removal() {
    let fixture = Fixture::new();
    let payload = StrokeAppendPayload::new(
        fixture.canvas_id.clone(),
        fixture.stroke_id.clone(),
        1,
        [point(2.0)],
    )
    .expect("append should be valid");
    let action = fixture.action("canvas.stroke.append", &payload);

    for lifecycle in [
        StrokeLifecycle::Active,
        StrokeLifecycle::Completed,
        StrokeLifecycle::Removed,
    ] {
        let drafts = service(
            &fixture,
            MockCanvasStateReader::found(fixture.projection(lifecycle, true)),
        )
        .append(&action, &payload)
        .await
        .expect("identical historical append should be idempotent");
        assert!(drafts.is_empty());
    }
}

#[tokio::test]
async fn append_rejects_new_chunk_after_completion_and_out_of_bounds_points() {
    let fixture = Fixture::new();
    let new_chunk = StrokeAppendPayload::new(
        fixture.canvas_id.clone(),
        fixture.stroke_id.clone(),
        2,
        [point(3.0)],
    )
    .expect("append should be valid");
    let lifecycle_error = service(
        &fixture,
        MockCanvasStateReader::found(fixture.projection(StrokeLifecycle::Completed, true)),
    )
    .append(
        &fixture.action("canvas.stroke.append", &new_chunk),
        &new_chunk,
    )
    .await
    .expect_err("completed Stroke should reject new chunk");
    assert!(matches!(
        lifecycle_error,
        CanvasRuntimeError::DomainViolation {
            source: CanvasError::InvalidStrokeState { .. }
        }
    ));

    let outside = StrokeAppendPayload::new(
        fixture.canvas_id.clone(),
        fixture.stroke_id.clone(),
        1,
        [CanvasPoint::new(120.0, 10.0).expect("point should be finite")],
    )
    .expect("append should be structurally valid");
    let bounds_error = service(
        &fixture,
        MockCanvasStateReader::found(fixture.projection(StrokeLifecycle::Active, false)),
    )
    .append(&fixture.action("canvas.stroke.append", &outside), &outside)
    .await
    .expect_err("out-of-bounds point should fail");
    assert!(matches!(
        bounds_error,
        CanvasRuntimeError::DomainViolation {
            source: CanvasError::InvalidCoordinate { .. }
        }
    ));
}

#[tokio::test]
async fn end_supports_transition_and_historical_retries() {
    let fixture = Fixture::new();
    let payload = StrokeEndPayload::new(fixture.canvas_id.clone(), fixture.stroke_id.clone(), 1);
    let action = fixture.action("canvas.stroke.end", &payload);

    let drafts = service(
        &fixture,
        MockCanvasStateReader::found(fixture.projection(StrokeLifecycle::Active, true)),
    )
    .end(&action, &payload)
    .await
    .expect("active Stroke should complete");
    assert_eq!(drafts[0].event_type().as_str(), STROKE_COMPLETED_EVENT_TYPE);

    for lifecycle in [StrokeLifecycle::Completed, StrokeLifecycle::Removed] {
        let drafts = service(
            &fixture,
            MockCanvasStateReader::found(fixture.projection(lifecycle, true)),
        )
        .end(&action, &payload)
        .await
        .expect("matching historical end should be idempotent");
        assert!(drafts.is_empty());
    }

    let rejected = service(
        &fixture,
        MockCanvasStateReader::found(fixture.projection(StrokeLifecycle::Cancelled, true)),
    )
    .end(&action, &payload)
    .await
    .expect_err("cancelled Stroke cannot complete");
    assert_invalid_state(rejected);

    let wrong = StrokeEndPayload::new(fixture.canvas_id.clone(), fixture.stroke_id.clone(), 0);
    let wrong_error = service(
        &fixture,
        MockCanvasStateReader::found(fixture.projection(StrokeLifecycle::Completed, true)),
    )
    .end(&fixture.action("canvas.stroke.end", &wrong), &wrong)
    .await
    .expect_err("wrong final index should fail");
    assert_invalid_index(wrong_error);
}

#[tokio::test]
async fn cancel_supports_transition_and_only_cancel_retry() {
    let fixture = Fixture::new();
    let payload = StrokeCancelPayload::new(fixture.canvas_id.clone(), fixture.stroke_id.clone(), 1);
    let action = fixture.action("canvas.stroke.cancel", &payload);

    let drafts = service(
        &fixture,
        MockCanvasStateReader::found(fixture.projection(StrokeLifecycle::Active, true)),
    )
    .cancel(&action, &payload)
    .await
    .expect("active Stroke should cancel");
    assert_eq!(drafts[0].event_type().as_str(), STROKE_CANCELLED_EVENT_TYPE);

    let retry = service(
        &fixture,
        MockCanvasStateReader::found(fixture.projection(StrokeLifecycle::Cancelled, true)),
    )
    .cancel(&action, &payload)
    .await
    .expect("matching cancel retry should be idempotent");
    assert!(retry.is_empty());

    for lifecycle in [StrokeLifecycle::Completed, StrokeLifecycle::Removed] {
        let error = service(
            &fixture,
            MockCanvasStateReader::found(fixture.projection(lifecycle, true)),
        )
        .cancel(&action, &payload)
        .await
        .expect_err("terminal non-cancelled Stroke should reject cancellation");
        assert_invalid_state(error);
    }

    let wrong = StrokeCancelPayload::new(fixture.canvas_id.clone(), fixture.stroke_id.clone(), 0);
    let wrong_error = service(
        &fixture,
        MockCanvasStateReader::found(fixture.projection(StrokeLifecycle::Cancelled, true)),
    )
    .cancel(&fixture.action("canvas.stroke.cancel", &wrong), &wrong)
    .await
    .expect_err("wrong cancel final index should fail");
    assert_invalid_index(wrong_error);
}

#[tokio::test]
async fn remove_supports_completed_transition_and_removed_retry() {
    let fixture = Fixture::new();
    let payload = StrokeRemovePayload::new(fixture.canvas_id.clone(), fixture.stroke_id.clone());
    let action = fixture.action("canvas.stroke.remove", &payload);

    let drafts = service(
        &fixture,
        MockCanvasStateReader::found(fixture.projection(StrokeLifecycle::Completed, true)),
    )
    .remove(&action, &payload)
    .await
    .expect("completed Stroke should remove");
    assert_eq!(drafts[0].event_type().as_str(), STROKE_REMOVED_EVENT_TYPE);

    let retry = service(
        &fixture,
        MockCanvasStateReader::found(fixture.projection(StrokeLifecycle::Removed, true)),
    )
    .remove(&action, &payload)
    .await
    .expect("removed retry should be idempotent");
    assert!(retry.is_empty());

    for lifecycle in [StrokeLifecycle::Active, StrokeLifecycle::Cancelled] {
        let error = service(
            &fixture,
            MockCanvasStateReader::found(fixture.projection(lifecycle, true)),
        )
        .remove(&action, &payload)
        .await
        .expect_err("non-completed Stroke should reject removal");
        assert_invalid_state(error);
    }
}

#[tokio::test]
async fn state_failures_and_corruption_are_not_reported_as_not_found() {
    let fixture = Fixture::new();
    let payload = fixture.begin_payload();
    let action = fixture.action("canvas.stroke.begin", &payload);

    let failed = service(&fixture, MockCanvasStateReader::failed())
        .begin(&action, &payload)
        .await
        .expect_err("state failure should propagate");
    assert!(matches!(failed, CanvasRuntimeError::StateReadFailed { .. }));

    let corrupted = service(
        &fixture,
        MockCanvasStateReader::corrupted(CanvasProjectionError::ProjectionMissing {
            event_type: orbitrelay_protocol::EventType::new("canvas.stroke.completed"),
        }),
    )
    .begin(&action, &payload)
    .await
    .expect_err("corrupted history should propagate");
    assert!(matches!(
        corrupted,
        CanvasRuntimeError::ProjectionCorrupted { .. }
    ));
}

fn assert_stroke_already_exists(error: CanvasRuntimeError) {
    assert!(matches!(
        error,
        CanvasRuntimeError::DomainViolation {
            source: CanvasError::StrokeAlreadyExists { .. }
        }
    ));
}

fn assert_invalid_state(error: CanvasRuntimeError) {
    assert!(matches!(
        error,
        CanvasRuntimeError::DomainViolation {
            source: CanvasError::InvalidStrokeState { .. }
        }
    ));
}

fn assert_invalid_index(error: CanvasRuntimeError) {
    assert!(matches!(
        error,
        CanvasRuntimeError::DomainViolation {
            source: CanvasError::InvalidChunkIndex { .. }
        }
    ));
}
