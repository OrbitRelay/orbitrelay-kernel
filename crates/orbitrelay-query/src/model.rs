//! Generic Query wire-neutral values.

use std::{fmt, str::FromStr};

use orbitrelay_protocol::{ActorId, ActorType, MessageId, Payload};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A validated, extensible dot-separated Query type such as `document.get` or
/// `asset.access.resolve`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct QueryType(String);

impl QueryType {
    /// Creates a Query type after validating `namespace.name` syntax.
    pub fn new(value: impl Into<String>) -> Result<Self, QueryTypeError> {
        let value = value.into();
        validate_query_type(&value)?;
        Ok(Self(value))
    }

    /// Returns the validated Query type string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for QueryType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for QueryType {
    type Err = QueryTypeError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for QueryType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

fn validate_query_type(value: &str) -> Result<(), QueryTypeError> {
    if value.trim() != value || value.is_empty() {
        return Err(QueryTypeError::InvalidFormat);
    }
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() < 2
        || parts.iter().any(|part| {
            part.is_empty()
                || !part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        })
    {
        return Err(QueryTypeError::InvalidFormat);
    }
    Ok(())
}

/// Query type construction failure.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum QueryTypeError {
    /// The value was empty, whitespace-padded, or not a namespaced identifier.
    #[error("query type must use non-empty dot-separated namespaced syntax")]
    InvalidFormat,
}

/// A trusted actor context supplied by an authenticated connection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryActorContext {
    actor_id: ActorId,
    actor_type: Option<ActorType>,
}

impl QueryActorContext {
    /// Creates a context when the transport has only an actor identity.
    #[must_use]
    pub const fn new(actor_id: ActorId) -> Self {
        Self {
            actor_id,
            actor_type: None,
        }
    }

    /// Creates a context with an optional infrastructure actor type.
    #[must_use]
    pub const fn with_type(actor_id: ActorId, actor_type: ActorType) -> Self {
        Self {
            actor_id,
            actor_type: Some(actor_type),
        }
    }

    /// Returns the trusted actor identity.
    #[must_use]
    pub const fn actor_id(&self) -> &ActorId {
        &self.actor_id
    }

    /// Returns the actor type when the authenticator supplied one.
    #[must_use]
    pub const fn actor_type(&self) -> Option<ActorType> {
        self.actor_type
    }
}

/// A transport-neutral Query request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryRequest {
    request_id: MessageId,
    query_type: QueryType,
    payload: Payload,
}

impl QueryRequest {
    /// Creates a Query request using its MessageId as request identity.
    #[must_use]
    pub const fn new(request_id: MessageId, query_type: QueryType, payload: Payload) -> Self {
        Self {
            request_id,
            query_type,
            payload,
        }
    }

    /// Returns the request/correlation identity.
    #[must_use]
    pub const fn request_id(&self) -> &MessageId {
        &self.request_id
    }

    /// Returns the requested operation.
    #[must_use]
    pub const fn query_type(&self) -> &QueryType {
        &self.query_type
    }

    /// Returns the strict JSON object payload.
    #[must_use]
    pub const fn payload(&self) -> &Payload {
        &self.payload
    }
}

/// Stable external Query failure categories.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryFailureCode {
    /// No registered handler exists for the Query type.
    UnsupportedQuery,
    /// The typed Query payload failed validation.
    InvalidQuery,
    /// The trusted actor is not allowed to read the target.
    Unauthorized,
    /// The requested read resource does not exist.
    NotFound,
    /// A read backend or handler failed.
    Internal,
    /// A backend may become available after retrying.
    Unavailable,
    /// The connection or application is not ready for this Query.
    NotReady,
}

/// A safe, stable application failure returned in `QueryResponse`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryFailure {
    code: QueryFailureCode,
    message: String,
    retryable: bool,
}

impl QueryFailure {
    /// Creates an application failure with a caller-safe message.
    #[must_use]
    pub fn new(code: QueryFailureCode, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code,
            message: message.into(),
            retryable,
        }
    }

    /// Returns the stable failure code.
    #[must_use]
    pub const fn code(&self) -> QueryFailureCode {
        self.code
    }

    /// Returns the client-safe failure message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns whether a retry may succeed.
    #[must_use]
    pub const fn retryable(&self) -> bool {
        self.retryable
    }
}

/// A successful or failed Query result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", content = "payload", rename_all = "snake_case")]
pub enum QueryResult {
    /// Typed handler output encoded as a JSON object payload.
    Success(Payload),
    /// Stable application failure.
    Error(QueryFailure),
}

/// A response correlated to its original Query request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryResponse {
    request_id: MessageId,
    query_type: QueryType,
    result: QueryResult,
}

impl QueryResponse {
    /// Creates a successful response.
    #[must_use]
    pub const fn success(request_id: MessageId, query_type: QueryType, payload: Payload) -> Self {
        Self {
            request_id,
            query_type,
            result: QueryResult::Success(payload),
        }
    }

    /// Creates an error response.
    #[must_use]
    pub const fn error(
        request_id: MessageId,
        query_type: QueryType,
        failure: QueryFailure,
    ) -> Self {
        Self {
            request_id,
            query_type,
            result: QueryResult::Error(failure),
        }
    }

    /// Returns the request identity being correlated.
    #[must_use]
    pub const fn request_id(&self) -> &MessageId {
        &self.request_id
    }

    /// Returns the original Query type.
    #[must_use]
    pub const fn query_type(&self) -> &QueryType {
        &self.query_type
    }

    /// Returns the success or failure result.
    #[must_use]
    pub const fn result(&self) -> &QueryResult {
        &self.result
    }

    /// Consumes the response and returns its result.
    #[must_use]
    pub fn into_result(self) -> QueryResult {
        self.result
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use orbitrelay_protocol::{ActorId, MessageId, Payload};
    use serde_json::json;

    use super::{
        QueryActorContext, QueryFailure, QueryFailureCode, QueryRequest, QueryResponse,
        QueryResult, QueryType,
    };

    #[test]
    fn query_type_accepts_namespaced_values_and_rejects_malformed_values() {
        for value in [
            "document.list",
            "plugin.v1",
            "quiz._search2",
            "asset.access.resolve",
        ] {
            assert!(
                QueryType::from_str(value).is_ok(),
                "{value} should be valid"
            );
        }
        for value in [
            "",
            " ",
            " document.list",
            "document.list ",
            "document",
            "Document.list",
            "document-list",
        ] {
            assert!(
                QueryType::from_str(value).is_err(),
                "{value:?} should be invalid"
            );
        }
    }

    #[test]
    fn request_and_response_round_trip_without_a_second_identity() {
        let request_id = MessageId::new();
        let query_type = QueryType::new("document.list").expect("valid query type");
        let mut payload = Payload::new();
        payload.insert("session_id", json!("session"));
        let request = QueryRequest::new(request_id.clone(), query_type.clone(), payload);
        let encoded = serde_json::to_vec(&request).expect("request should encode");
        let decoded: QueryRequest =
            serde_json::from_slice(&encoded).expect("request should decode");
        assert_eq!(decoded, request);

        let response = QueryResponse::error(
            request_id.clone(),
            query_type.clone(),
            QueryFailure::new(QueryFailureCode::Unauthorized, "denied", false),
        );
        let encoded = serde_json::to_vec(&response).expect("response should encode");
        let decoded: QueryResponse =
            serde_json::from_slice(&encoded).expect("response should decode");
        assert_eq!(decoded, response);
        assert_eq!(decoded.request_id(), &request_id);
        assert_eq!(decoded.query_type(), &query_type);
        assert!(
            matches!(decoded.result(), QueryResult::Error(failure) if failure.code() == QueryFailureCode::Unauthorized)
        );
    }

    #[test]
    fn actor_context_is_transport_neutral() {
        let actor = ActorId::new();
        let context = QueryActorContext::new(actor.clone());
        assert_eq!(context.actor_id(), &actor);
        assert_eq!(context.actor_type(), None);
    }
}
