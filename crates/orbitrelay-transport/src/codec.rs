//! Message codec abstraction and strict JSON implementation.

use serde_json::{Map, Value};

use crate::{CodecError, InboundMessage, OutboundMessage};

const JSON_CODEC_NAME: &str = "json";

/// Encodes and decodes transport messages without managing connection state.
pub trait MessageCodec: Send + Sync {
    /// Returns the codec name used during transport negotiation.
    fn name(&self) -> &'static str;

    /// Decodes one client message.
    fn decode_inbound(&self, bytes: &[u8]) -> Result<InboundMessage, CodecError>;

    /// Encodes one server message.
    fn encode_outbound(&self, message: &OutboundMessage) -> Result<Vec<u8>, CodecError>;
}

/// A strict UTF-8 JSON codec for the initial transport protocol.
///
/// Unknown fields are rejected so misspellings and incompatible client
/// assumptions fail visibly instead of being silently ignored.
#[derive(Clone, Copy, Debug, Default)]
pub struct JsonCodec;

impl MessageCodec for JsonCodec {
    fn name(&self) -> &'static str {
        JSON_CODEC_NAME
    }

    fn decode_inbound(&self, bytes: &[u8]) -> Result<InboundMessage, CodecError> {
        let value: Value = serde_json::from_slice(bytes).map_err(|_| CodecError::InvalidJson)?;
        validate_inbound(&value)?;
        serde_json::from_value(value).map_err(|_| CodecError::DecodeFailed)
    }

    fn encode_outbound(&self, message: &OutboundMessage) -> Result<Vec<u8>, CodecError> {
        serde_json::to_vec(message).map_err(|_| CodecError::EncodeFailed)
    }
}

fn validate_inbound(value: &Value) -> Result<(), CodecError> {
    let object = require_object(value)?;
    validate_exact_fields(object, &["kind", "payload"])?;

    let kind = object
        .get("kind")
        .and_then(Value::as_str)
        .ok_or(CodecError::InvalidMessageShape)?;
    let payload = object
        .get("payload")
        .ok_or(CodecError::MissingField { field: "payload" })?;

    match kind {
        "hello" | "authenticate" | "subscribe" | "unsubscribe" | "ping" | "close" => Ok(()),
        "action" => validate_action_envelope(payload),
        "query" => validate_query_message(payload),
        unsupported => Err(CodecError::UnsupportedMessageType {
            message_type: unsupported.to_owned(),
        }),
    }
}

fn validate_query_message(value: &Value) -> Result<(), CodecError> {
    let message = require_object(value)?;
    validate_exact_fields(
        message,
        &["version", "message_id", "message_type", "payload"],
    )?;
    let version = message
        .get("version")
        .ok_or(CodecError::MissingField { field: "version" })?;
    validate_exact_fields(require_object(version)?, &["major", "minor", "patch"])?;
    let query_type = message
        .get("message_type")
        .and_then(Value::as_str)
        .ok_or(CodecError::InvalidMessageShape)?;
    orbitrelay_query::QueryType::new(query_type.to_owned())
        .map_err(|_| CodecError::InvalidMessageShape)?;
    require_object(
        message
            .get("payload")
            .ok_or(CodecError::MissingField { field: "payload" })?,
    )?;
    Ok(())
}

fn validate_action_envelope(value: &Value) -> Result<(), CodecError> {
    let envelope = require_object(value)?;
    validate_exact_fields(
        envelope,
        &["version", "message_id", "message_type", "payload"],
    )?;

    let version = envelope
        .get("version")
        .ok_or(CodecError::MissingField { field: "version" })?;
    validate_exact_fields(require_object(version)?, &["major", "minor", "patch"])?;

    let message_type = envelope
        .get("message_type")
        .and_then(Value::as_str)
        .ok_or(CodecError::InvalidMessageShape)?;
    if message_type != "action" {
        return Err(CodecError::UnexpectedEnvelopeType {
            expected: "action",
            actual: message_type.to_owned(),
        });
    }

    let action = envelope
        .get("payload")
        .ok_or(CodecError::MissingField { field: "payload" })?;
    let action = require_object(action)?;
    validate_exact_fields(
        action,
        &[
            "id",
            "session_id",
            "actor_id",
            "action_type",
            "requested_at",
            "payload",
            "metadata",
        ],
    )?;

    require_object(
        action
            .get("payload")
            .ok_or(CodecError::MissingField { field: "payload" })?,
    )?;
    require_object(
        action
            .get("metadata")
            .ok_or(CodecError::MissingField { field: "metadata" })?,
    )?;
    Ok(())
}

fn require_object(value: &Value) -> Result<&Map<String, Value>, CodecError> {
    value.as_object().ok_or(CodecError::InvalidMessageShape)
}

fn validate_exact_fields(
    object: &Map<String, Value>,
    expected: &[&'static str],
) -> Result<(), CodecError> {
    for field in expected {
        if !object.contains_key(*field) {
            return Err(CodecError::MissingField { field });
        }
    }
    for field in object.keys() {
        if !expected.contains(&field.as_str()) {
            return Err(CodecError::UnknownField {
                field: field.clone(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn test_encode_inbound(message: &InboundMessage) -> Vec<u8> {
    serde_json::to_vec(message).expect("test inbound message should serialize")
}

#[cfg(test)]
pub(crate) fn test_decode_outbound(bytes: &[u8]) -> OutboundMessage {
    serde_json::from_slice(bytes).expect("test outbound message should deserialize")
}

#[cfg(test)]
mod tests {
    use orbitrelay_core::{Metadata, Timestamp, Version};
    use orbitrelay_protocol::{
        Action, ActionId, ActionType, ActorId, Event, EventId, EventType, MessageEnvelope,
        MessageId, MessageType, Payload, SessionId,
    };
    use serde_json::json;

    use super::{test_decode_outbound, JsonCodec, MessageCodec};
    use crate::{
        ErrorMessage, InboundMessage, OutboundMessage, SubscriptionRequest, TransportError,
        TransportErrorCode, TransportExecutionError, CURRENT_PROTOCOL_VERSION,
    };

    fn action() -> Action {
        let mut payload = Payload::new();
        payload.insert("x", json!(12));
        Action::new(
            ActionId::new(),
            SessionId::new(),
            ActorId::new(),
            ActionType::new("canvas.draw"),
            Timestamp::from_unix_timestamp(1_700_000_000).expect("valid timestamp"),
            payload,
            Metadata::new(),
        )
    }

    fn event() -> Event {
        Event::new(
            EventId::new(),
            SessionId::new(),
            ActorId::new(),
            ActionId::new(),
            EventType::new("canvas.drawn"),
            Timestamp::from_unix_timestamp(1_700_000_001).expect("valid timestamp"),
            Payload::new(),
            Metadata::new(),
        )
    }

    #[test]
    fn action_message_round_trips_through_json_decoder() {
        let message = InboundMessage::Action(MessageEnvelope::new(
            CURRENT_PROTOCOL_VERSION,
            MessageId::new(),
            MessageType::new("action"),
            action(),
        ));
        let bytes = serde_json::to_vec(&message).expect("test message should serialize");
        let decoded = JsonCodec
            .decode_inbound(&bytes)
            .expect("valid action should decode");

        assert_eq!(decoded, message);
    }

    #[test]
    fn event_message_round_trips_through_json_encoder() {
        let message = OutboundMessage::Event(MessageEnvelope::new(
            CURRENT_PROTOCOL_VERSION,
            MessageId::new(),
            MessageType::new("event"),
            event(),
        ));
        let bytes = JsonCodec
            .encode_outbound(&message)
            .expect("valid event should encode");
        let decoded: OutboundMessage =
            serde_json::from_slice(&bytes).expect("encoded event should decode");

        assert_eq!(decoded, message);
    }

    #[test]
    fn rejects_invalid_json() {
        let result = JsonCodec.decode_inbound(br#"{"kind": "action""#);

        assert!(result.is_err());
    }

    #[test]
    fn rejects_unknown_message_kind() {
        let result = JsonCodec.decode_inbound(br#"{"kind":"future","payload":{}}"#);

        assert!(result.is_err());
    }

    #[test]
    fn rejects_missing_control_field() {
        let result =
            JsonCodec.decode_inbound(br#"{"kind":"hello","payload":{"supported_versions":[]}}"#);

        assert!(result.is_err());
    }

    #[test]
    fn rejects_unknown_control_field() {
        let result =
            JsonCodec.decode_inbound(br#"{"kind":"ping","payload":{"nonce":1,"extra":true}}"#);

        assert!(result.is_err());
    }

    #[test]
    fn rejects_unknown_top_level_field() {
        let result =
            JsonCodec.decode_inbound(br#"{"kind":"ping","payload":{"nonce":1},"extra":true}"#);

        assert!(result.is_err());
    }

    #[test]
    fn subscription_request_serializes_with_stable_event_order() {
        let request = SubscriptionRequest::new(
            MessageId::new(),
            SessionId::new(),
            [EventType::new("z.event"), EventType::new("a.event")],
        );
        let encoded = serde_json::to_string(&request).expect("request should serialize");
        let first = encoded.find("a.event").expect("first event should exist");
        let second = encoded.find("z.event").expect("second event should exist");

        assert!(first < second);
    }

    #[test]
    fn transport_error_code_has_stable_snake_case_encoding() {
        let encoded = serde_json::to_string(&TransportErrorCode::IdentityMismatch)
            .expect("error code should serialize");

        assert_eq!(encoded, "\"identity_mismatch\"");
    }

    #[test]
    fn encoded_error_message_does_not_leak_internal_detail() {
        let secret = "backend credential secret";
        let error = TransportError::Execution(TransportExecutionError::Failed {
            detail: secret.to_owned(),
        });
        let outbound = OutboundMessage::Error(ErrorMessage::from_transport_error(
            Some(MessageId::new()),
            &error,
        ));
        let bytes = JsonCodec
            .encode_outbound(&outbound)
            .expect("safe error should encode");
        let encoded = String::from_utf8(bytes).expect("JSON should be UTF-8");

        assert!(!encoded.contains(secret));
        assert!(encoded.contains("internal_error"));
    }

    #[test]
    fn json_codec_reports_its_negotiation_name() {
        assert_eq!(JsonCodec.name(), "json");
        assert_ne!(CURRENT_PROTOCOL_VERSION, Version::new(0, 2, 0));
    }

    #[test]
    fn protocol_02_query_fixture_decodes_without_domain_parsing() {
        let list = include_bytes!("../../../tests/fixtures/v0.2/query_document_list.json");
        let get = include_bytes!("../../../tests/fixtures/v0.2/query_document_get.json");
        assert!(matches!(
            JsonCodec.decode_inbound(list),
            Ok(InboundMessage::Query(_))
        ));
        assert!(matches!(
            JsonCodec.decode_inbound(get),
            Ok(InboundMessage::Query(_))
        ));
    }

    #[test]
    fn protocol_02_query_response_fixtures_decode_as_outbound_messages() {
        for fixture in [
            include_bytes!(
                "../../../tests/fixtures/v0.2/query_response_document_list_success.json"
            )
            .as_slice(),
            include_bytes!("../../../tests/fixtures/v0.2/query_response_document_get_success.json")
                .as_slice(),
            include_bytes!("../../../tests/fixtures/v0.2/query_response_not_found.json").as_slice(),
            include_bytes!("../../../tests/fixtures/v0.2/query_response_unauthorized.json")
                .as_slice(),
        ] {
            let message: OutboundMessage = serde_json::from_slice(fixture)
                .expect("v0.2 fixture should decode as an outbound message");
            assert!(matches!(message, OutboundMessage::QueryResponse(_)));
            let encoded = JsonCodec
                .encode_outbound(&message)
                .expect("v0.2 response should encode");
            let decoded = test_decode_outbound(&encoded);
            assert_eq!(decoded, message);
        }
    }
}
