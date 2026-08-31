//! Strict conversion between generic protocol payloads and Canvas values.

use orbitrelay_protocol::Payload;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{CanvasError, CanvasId, CanvasPoint, LayerId, StrokeId, StrokeStyle, StrokeTool};

/// Maximum logical points accepted in one begin or append chunk.
pub const MAX_POINTS_PER_CHUNK: usize = 256;

/// Strong payload for `canvas.stroke.begin` and `canvas.stroke.began`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrokeBeginPayload {
    canvas_id: CanvasId,
    layer_id: LayerId,
    stroke_id: StrokeId,
    tool: StrokeTool,
    style: StrokeStyle,
    chunk_index: u64,
    points: Vec<CanvasPoint>,
}

impl StrokeBeginPayload {
    /// Creates a validated initial Stroke chunk.
    pub fn new(
        canvas_id: CanvasId,
        layer_id: LayerId,
        stroke_id: StrokeId,
        tool: StrokeTool,
        style: StrokeStyle,
        chunk_index: u64,
        points: impl IntoIterator<Item = CanvasPoint>,
    ) -> Result<Self, CanvasError> {
        let payload = Self {
            canvas_id,
            layer_id,
            stroke_id,
            tool,
            style,
            chunk_index,
            points: points.into_iter().collect(),
        };
        payload.validate()?;
        Ok(payload)
    }

    /// Validates the begin chunk index and point count.
    pub fn validate(&self) -> Result<(), CanvasError> {
        validate_begin_chunk_index(self.chunk_index)?;
        validate_points(&self.points)
    }

    /// Returns the target Canvas.
    #[must_use]
    pub const fn canvas_id(&self) -> &CanvasId {
        &self.canvas_id
    }

    /// Returns the target Canvas layer.
    #[must_use]
    pub const fn layer_id(&self) -> &LayerId {
        &self.layer_id
    }

    /// Returns the stable Stroke identifier.
    #[must_use]
    pub const fn stroke_id(&self) -> &StrokeId {
        &self.stroke_id
    }

    /// Returns the Stroke tool.
    #[must_use]
    pub const fn tool(&self) -> StrokeTool {
        self.tool
    }

    /// Returns the logical Stroke style.
    #[must_use]
    pub const fn style(&self) -> &StrokeStyle {
        &self.style
    }

    /// Returns the initial chunk index, which is always zero after validation.
    #[must_use]
    pub const fn chunk_index(&self) -> u64 {
        self.chunk_index
    }

    /// Returns the initial point batch in drawing order.
    #[must_use]
    pub fn points(&self) -> &[CanvasPoint] {
        &self.points
    }
}

/// Strong payload for `canvas.stroke.append` and `canvas.stroke.points_appended`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrokeAppendPayload {
    canvas_id: CanvasId,
    stroke_id: StrokeId,
    chunk_index: u64,
    points: Vec<CanvasPoint>,
}

impl StrokeAppendPayload {
    /// Creates a validated non-initial Stroke point chunk.
    pub fn new(
        canvas_id: CanvasId,
        stroke_id: StrokeId,
        chunk_index: u64,
        points: impl IntoIterator<Item = CanvasPoint>,
    ) -> Result<Self, CanvasError> {
        let payload = Self {
            canvas_id,
            stroke_id,
            chunk_index,
            points: points.into_iter().collect(),
        };
        payload.validate()?;
        Ok(payload)
    }

    /// Validates the append chunk index and point count.
    pub fn validate(&self) -> Result<(), CanvasError> {
        validate_append_chunk_index(self.chunk_index)?;
        validate_points(&self.points)
    }

    /// Returns the target Canvas.
    #[must_use]
    pub const fn canvas_id(&self) -> &CanvasId {
        &self.canvas_id
    }

    /// Returns the stable Stroke identifier.
    #[must_use]
    pub const fn stroke_id(&self) -> &StrokeId {
        &self.stroke_id
    }

    /// Returns the positive point chunk index.
    #[must_use]
    pub const fn chunk_index(&self) -> u64 {
        self.chunk_index
    }

    /// Returns the appended point batch in drawing order.
    #[must_use]
    pub fn points(&self) -> &[CanvasPoint] {
        &self.points
    }
}

/// Strong payload for `canvas.stroke.end` and `canvas.stroke.completed`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrokeEndPayload {
    canvas_id: CanvasId,
    stroke_id: StrokeId,
    final_chunk_index: u64,
}

impl StrokeEndPayload {
    /// Creates a terminal completion declaration.
    #[must_use]
    pub const fn new(canvas_id: CanvasId, stroke_id: StrokeId, final_chunk_index: u64) -> Self {
        Self {
            canvas_id,
            stroke_id,
            final_chunk_index,
        }
    }

    /// Validates the structurally unrestricted terminal index.
    pub const fn validate(&self) -> Result<(), CanvasError> {
        Ok(())
    }

    /// Returns the target Canvas.
    #[must_use]
    pub const fn canvas_id(&self) -> &CanvasId {
        &self.canvas_id
    }

    /// Returns the stable Stroke identifier.
    #[must_use]
    pub const fn stroke_id(&self) -> &StrokeId {
        &self.stroke_id
    }

    /// Returns the client-declared final chunk index, including zero.
    #[must_use]
    pub const fn final_chunk_index(&self) -> u64 {
        self.final_chunk_index
    }
}

/// Strong payload for `canvas.stroke.cancel` and `canvas.stroke.cancelled`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrokeCancelPayload {
    canvas_id: CanvasId,
    stroke_id: StrokeId,
    final_chunk_index: u64,
}

impl StrokeCancelPayload {
    /// Creates a terminal cancellation declaration.
    #[must_use]
    pub const fn new(canvas_id: CanvasId, stroke_id: StrokeId, final_chunk_index: u64) -> Self {
        Self {
            canvas_id,
            stroke_id,
            final_chunk_index,
        }
    }

    /// Validates the structurally unrestricted terminal index.
    pub const fn validate(&self) -> Result<(), CanvasError> {
        Ok(())
    }

    /// Returns the target Canvas.
    #[must_use]
    pub const fn canvas_id(&self) -> &CanvasId {
        &self.canvas_id
    }

    /// Returns the stable Stroke identifier.
    #[must_use]
    pub const fn stroke_id(&self) -> &StrokeId {
        &self.stroke_id
    }

    /// Returns the client-declared final chunk index, including zero.
    #[must_use]
    pub const fn final_chunk_index(&self) -> u64 {
        self.final_chunk_index
    }
}

/// Strong payload for `canvas.stroke.remove` and `canvas.stroke.removed`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrokeRemovePayload {
    canvas_id: CanvasId,
    stroke_id: StrokeId,
}

impl StrokeRemovePayload {
    /// Creates a request or fact identifying a Stroke to remove.
    #[must_use]
    pub const fn new(canvas_id: CanvasId, stroke_id: StrokeId) -> Self {
        Self {
            canvas_id,
            stroke_id,
        }
    }

    /// Validates the structurally complete removal payload.
    pub const fn validate(&self) -> Result<(), CanvasError> {
        Ok(())
    }

    /// Returns the target Canvas.
    #[must_use]
    pub const fn canvas_id(&self) -> &CanvasId {
        &self.canvas_id
    }

    /// Returns the stable Stroke identifier.
    #[must_use]
    pub const fn stroke_id(&self) -> &StrokeId {
        &self.stroke_id
    }
}

/// Validates that a begin chunk uses index zero.
pub fn validate_begin_chunk_index(chunk_index: u64) -> Result<(), CanvasError> {
    if chunk_index == 0 {
        Ok(())
    } else {
        Err(CanvasError::InvalidChunkIndex {
            kind: "begin",
            chunk_index,
        })
    }
}

/// Validates that an append chunk uses a positive index.
pub fn validate_append_chunk_index(chunk_index: u64) -> Result<(), CanvasError> {
    if chunk_index >= 1 {
        Ok(())
    } else {
        Err(CanvasError::InvalidChunkIndex {
            kind: "append",
            chunk_index,
        })
    }
}

fn validate_points(points: &[CanvasPoint]) -> Result<(), CanvasError> {
    if (1..=MAX_POINTS_PER_CHUNK).contains(&points.len()) {
        Ok(())
    } else {
        Err(CanvasError::InvalidPointCount {
            actual: points.len(),
            maximum: MAX_POINTS_PER_CHUNK,
        })
    }
}

trait ValidatedPayload: Serialize + DeserializeOwned {
    fn validate_payload(&self) -> Result<(), CanvasError>;
}

macro_rules! impl_validated_payload {
    ($payload:ty) => {
        impl ValidatedPayload for $payload {
            fn validate_payload(&self) -> Result<(), CanvasError> {
                self.validate()
            }
        }

        impl TryFrom<&Payload> for $payload {
            type Error = CanvasError;

            fn try_from(payload: &Payload) -> Result<Self, Self::Error> {
                decode_payload(payload)
            }
        }

        impl TryFrom<Payload> for $payload {
            type Error = CanvasError;

            fn try_from(payload: Payload) -> Result<Self, Self::Error> {
                Self::try_from(&payload)
            }
        }

        impl TryFrom<&$payload> for Payload {
            type Error = CanvasError;

            fn try_from(payload: &$payload) -> Result<Self, Self::Error> {
                encode_payload(payload)
            }
        }

        impl TryFrom<$payload> for Payload {
            type Error = CanvasError;

            fn try_from(payload: $payload) -> Result<Self, Self::Error> {
                Self::try_from(&payload)
            }
        }
    };
}

impl_validated_payload!(StrokeBeginPayload);
impl_validated_payload!(StrokeAppendPayload);
impl_validated_payload!(StrokeEndPayload);
impl_validated_payload!(StrokeCancelPayload);
impl_validated_payload!(StrokeRemovePayload);

fn decode_payload<T>(payload: &Payload) -> Result<T, CanvasError>
where
    T: ValidatedPayload,
{
    let value = serde_json::to_value(payload).map_err(|_| invalid_payload("decode failed"))?;
    let decoded: T =
        serde_json::from_value(value).map_err(|_| invalid_payload("schema mismatch"))?;
    decoded.validate_payload()?;
    Ok(decoded)
}

fn encode_payload<T>(payload: &T) -> Result<Payload, CanvasError>
where
    T: ValidatedPayload,
{
    payload.validate_payload()?;
    let value = serde_json::to_value(payload).map_err(|_| invalid_payload("encode failed"))?;
    serde_json::from_value(value).map_err(|_| invalid_payload("top level must be an object"))
}

fn invalid_payload(detail: &'static str) -> CanvasError {
    CanvasError::InvalidPayload {
        detail: detail.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use orbitrelay_protocol::Payload;
    use serde_json::{json, Value};

    use super::{
        StrokeAppendPayload, StrokeBeginPayload, StrokeCancelPayload, StrokeEndPayload,
        StrokeRemovePayload, MAX_POINTS_PER_CHUNK,
    };
    use crate::{
        CanvasError, CanvasId, CanvasPoint, LayerId, RgbaColor, StrokeId, StrokeStyle, StrokeTool,
    };

    fn point() -> CanvasPoint {
        CanvasPoint::new(10.0, 20.0).expect("point should be valid")
    }

    fn style() -> StrokeStyle {
        StrokeStyle::new(2.0, RgbaColor::new(10, 20, 30, 255)).expect("style should be valid")
    }

    fn begin_payload() -> StrokeBeginPayload {
        StrokeBeginPayload::new(
            CanvasId::new(),
            LayerId::new(),
            StrokeId::new(),
            StrokeTool::Pen,
            style(),
            0,
            [point()],
        )
        .expect("begin payload should be valid")
    }

    fn assert_payload_round_trip<T>(value: T)
    where
        T: Clone + std::fmt::Debug + PartialEq + TryFrom<Payload, Error = CanvasError>,
        Payload: TryFrom<T, Error = CanvasError>,
    {
        let encoded = Payload::try_from(value.clone()).expect("payload should encode");
        let decoded = T::try_from(encoded).expect("payload should decode");
        assert_eq!(decoded, value);
    }

    #[test]
    fn validates_begin_and_append_chunk_indexes() {
        assert!(StrokeBeginPayload::new(
            CanvasId::new(),
            LayerId::new(),
            StrokeId::new(),
            StrokeTool::Pen,
            style(),
            1,
            [point()]
        )
        .is_err());
        assert!(StrokeAppendPayload::new(CanvasId::new(), StrokeId::new(), 0, [point()]).is_err());
    }

    #[test]
    fn validates_point_batch_limits() {
        assert!(StrokeBeginPayload::new(
            CanvasId::new(),
            LayerId::new(),
            StrokeId::new(),
            StrokeTool::Pen,
            style(),
            0,
            []
        )
        .is_err());
        assert!(StrokeAppendPayload::new(CanvasId::new(), StrokeId::new(), 1, []).is_err());
        assert!(StrokeAppendPayload::new(
            CanvasId::new(),
            StrokeId::new(),
            1,
            vec![point(); MAX_POINTS_PER_CHUNK + 1]
        )
        .is_err());
        assert!(StrokeAppendPayload::new(
            CanvasId::new(),
            StrokeId::new(),
            1,
            vec![point(); MAX_POINTS_PER_CHUNK]
        )
        .is_ok());
    }

    #[test]
    fn all_payloads_round_trip_through_protocol_payload() {
        let begin = begin_payload();
        let canvas_id = begin.canvas_id().clone();
        let stroke_id = begin.stroke_id().clone();
        assert_payload_round_trip(begin);
        assert_payload_round_trip(
            StrokeAppendPayload::new(canvas_id.clone(), stroke_id.clone(), 1, [point()])
                .expect("append payload should be valid"),
        );
        assert_payload_round_trip(StrokeEndPayload::new(
            canvas_id.clone(),
            stroke_id.clone(),
            1,
        ));
        assert_payload_round_trip(StrokeCancelPayload::new(
            canvas_id.clone(),
            stroke_id.clone(),
            1,
        ));
        assert_payload_round_trip(StrokeRemovePayload::new(canvas_id, stroke_id));
    }

    #[test]
    fn borrowed_payload_conversion_round_trips() {
        let begin = begin_payload();
        let encoded = Payload::try_from(&begin).expect("borrowed payload should encode");
        let decoded =
            StrokeBeginPayload::try_from(&encoded).expect("borrowed payload should decode");

        assert_eq!(decoded, begin);
    }

    #[test]
    fn rejects_unknown_and_missing_fields() {
        let begin = begin_payload();
        let payload = Payload::try_from(begin).expect("payload should encode");
        let mut value = serde_json::to_value(payload).expect("payload should become JSON");
        value
            .as_object_mut()
            .expect("payload should be an object")
            .insert("unexpected".to_owned(), json!(true));
        let unknown: Payload =
            serde_json::from_value(value.clone()).expect("object should become Payload");
        assert!(matches!(
            StrokeBeginPayload::try_from(unknown),
            Err(CanvasError::InvalidPayload { .. })
        ));

        value
            .as_object_mut()
            .expect("payload should be an object")
            .remove("unexpected");
        value
            .as_object_mut()
            .expect("payload should be an object")
            .remove("stroke_id");
        let missing: Payload = serde_json::from_value(value).expect("object should become Payload");
        assert!(matches!(
            StrokeBeginPayload::try_from(missing),
            Err(CanvasError::InvalidPayload { .. })
        ));
    }

    #[test]
    fn semantic_validation_runs_after_deserialization() {
        let begin = begin_payload();
        let payload = Payload::try_from(begin).expect("payload should encode");
        let mut value = serde_json::to_value(payload).expect("payload should become JSON");
        value
            .as_object_mut()
            .expect("payload should be an object")
            .insert("chunk_index".to_owned(), Value::from(2));
        let invalid: Payload = serde_json::from_value(value).expect("object should become Payload");

        assert!(matches!(
            StrokeBeginPayload::try_from(invalid),
            Err(CanvasError::InvalidChunkIndex { .. })
        ));
    }
}
