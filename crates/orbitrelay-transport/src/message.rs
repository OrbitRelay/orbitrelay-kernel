//! Transport control-plane and protocol data-plane message models.

use orbitrelay_core::Version;
use orbitrelay_protocol::{Action, ActionId, Event, EventId, MessageEnvelope, MessageId, Payload};
use orbitrelay_query::{QueryRequest, QueryResponse, QueryResult, QueryType};
use serde::{Deserialize, Serialize};

use crate::{ErrorMessage, InboundCredentials, SubscriptionRequest, TransportSubscriptionId};

/// Client protocol and codec capabilities sent during negotiation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hello {
    supported_versions: Vec<Version>,
    codecs: Vec<String>,
}

impl Hello {
    /// Creates a negotiation request in client preference order.
    #[must_use]
    pub fn new(supported_versions: Vec<Version>, codecs: Vec<String>) -> Self {
        Self {
            supported_versions,
            codecs,
        }
    }

    /// Returns protocol versions supported by the client.
    #[must_use]
    pub fn supported_versions(&self) -> &[Version] {
        &self.supported_versions
    }

    /// Returns codec names supported by the client.
    #[must_use]
    pub fn codecs(&self) -> &[String] {
        &self.codecs
    }
}

/// Server selection returned after successful negotiation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelloAccepted {
    selected_version: Version,
    codec: String,
}

impl HelloAccepted {
    /// Creates a successful negotiation response.
    #[must_use]
    pub fn new(selected_version: Version, codec: impl Into<String>) -> Self {
        Self {
            selected_version,
            codec: codec.into(),
        }
    }

    /// Returns the selected protocol version.
    #[must_use]
    pub const fn selected_version(&self) -> Version {
        self.selected_version
    }

    /// Returns the selected codec name.
    #[must_use]
    pub fn codec(&self) -> &str {
        &self.codec
    }
}

/// A request to bind credentials to the current connection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Authenticate {
    request_id: MessageId,
    credentials: InboundCredentials,
}

impl Authenticate {
    /// Creates an authentication control message.
    #[must_use]
    pub const fn new(request_id: MessageId, credentials: InboundCredentials) -> Self {
        Self {
            request_id,
            credentials,
        }
    }

    /// Returns the control request identifier.
    #[must_use]
    pub const fn request_id(&self) -> &MessageId {
        &self.request_id
    }

    /// Returns the resolver credentials.
    #[must_use]
    pub const fn credentials(&self) -> &InboundCredentials {
        &self.credentials
    }
}

/// A request to stop one transport subscription.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Unsubscribe {
    request_id: MessageId,
    subscription_id: TransportSubscriptionId,
}

impl Unsubscribe {
    /// Creates an unsubscribe control message.
    #[must_use]
    pub const fn new(request_id: MessageId, subscription_id: TransportSubscriptionId) -> Self {
        Self {
            request_id,
            subscription_id,
        }
    }

    /// Returns the control request identifier.
    #[must_use]
    pub const fn request_id(&self) -> &MessageId {
        &self.request_id
    }

    /// Returns the subscription to close.
    #[must_use]
    pub const fn subscription_id(&self) -> &TransportSubscriptionId {
        &self.subscription_id
    }
}

/// A heartbeat request carrying an application-level nonce.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PingMessage {
    nonce: u64,
}

impl PingMessage {
    /// Creates a heartbeat request.
    #[must_use]
    pub const fn new(nonce: u64) -> Self {
        Self { nonce }
    }

    /// Returns the nonce to echo in the response.
    #[must_use]
    pub const fn nonce(self) -> u64 {
        self.nonce
    }
}

/// A heartbeat response carrying the request nonce.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PongMessage {
    nonce: u64,
}

/// A versioned inbound Query wire message.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryMessage {
    version: Version,
    message_id: MessageId,
    message_type: QueryType,
    payload: Payload,
}

impl QueryMessage {
    /// Creates a Query message using its MessageId as request identity.
    #[must_use]
    pub const fn new(
        version: Version,
        message_id: MessageId,
        message_type: QueryType,
        payload: Payload,
    ) -> Self {
        Self {
            version,
            message_id,
            message_type,
            payload,
        }
    }

    /// Returns the protocol version asserted by the request.
    #[must_use]
    pub const fn version(&self) -> Version {
        self.version
    }

    /// Returns the request MessageId.
    #[must_use]
    pub const fn message_id(&self) -> &MessageId {
        &self.message_id
    }

    /// Returns the namespaced Query type.
    #[must_use]
    pub const fn message_type(&self) -> &QueryType {
        &self.message_type
    }

    /// Returns the strict object payload.
    #[must_use]
    pub const fn payload(&self) -> &Payload {
        &self.payload
    }

    /// Converts the transport message to the generic Query request model.
    #[must_use]
    pub fn into_request(self) -> QueryRequest {
        QueryRequest::new(self.message_id, self.message_type, self.payload)
    }
}

/// A versioned outbound Query response wire message.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryResponseMessage {
    version: Version,
    request_id: MessageId,
    query_type: QueryType,
    result: QueryResult,
}

impl QueryResponseMessage {
    /// Creates a response from a generic Query response.
    #[must_use]
    pub fn from_response(version: Version, response: QueryResponse) -> Self {
        Self {
            version,
            request_id: response.request_id().clone(),
            query_type: response.query_type().clone(),
            result: response.into_result(),
        }
    }

    /// Returns the response protocol version.
    #[must_use]
    pub const fn version(&self) -> Version {
        self.version
    }

    /// Returns the correlated request MessageId.
    #[must_use]
    pub const fn request_id(&self) -> &MessageId {
        &self.request_id
    }

    /// Returns the original Query type.
    #[must_use]
    pub const fn query_type(&self) -> &QueryType {
        &self.query_type
    }

    /// Returns the success/error result.
    #[must_use]
    pub const fn result(&self) -> &QueryResult {
        &self.result
    }
}

impl PongMessage {
    /// Creates a heartbeat response.
    #[must_use]
    pub const fn new(nonce: u64) -> Self {
        Self { nonce }
    }

    /// Returns the echoed nonce.
    #[must_use]
    pub const fn nonce(self) -> u64 {
        self.nonce
    }
}

/// A transport-level request or notice to close a connection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CloseMessage {
    reason: Option<String>,
}

impl CloseMessage {
    /// Creates a close message with an optional safe reason.
    #[must_use]
    pub fn new(reason: Option<String>) -> Self {
        Self { reason }
    }

    /// Returns the safe close reason, when present.
    #[must_use]
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }
}

/// Acknowledges that an action completed execution at the transport boundary.
///
/// The acknowledgement does not mean that a client has consumed the generated
/// events. Event identifiers are included in execution order for correlation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionAcknowledgement {
    request_id: MessageId,
    action_id: ActionId,
    generated_event_ids: Vec<EventId>,
}

impl ActionAcknowledgement {
    /// Creates an acknowledgement after action execution completes.
    #[must_use]
    pub fn new(
        request_id: MessageId,
        action_id: ActionId,
        generated_event_ids: Vec<EventId>,
    ) -> Self {
        Self {
            request_id,
            action_id,
            generated_event_ids,
        }
    }

    /// Returns the original Action envelope message identifier.
    #[must_use]
    pub const fn request_id(&self) -> &MessageId {
        &self.request_id
    }

    /// Returns the executed action identifier.
    #[must_use]
    pub const fn action_id(&self) -> &ActionId {
        &self.action_id
    }

    /// Returns generated event identifiers in execution order.
    #[must_use]
    pub fn generated_event_ids(&self) -> &[EventId] {
        &self.generated_event_ids
    }
}

/// Confirms creation of an authorized transport subscription.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubscriptionAccepted {
    request_id: MessageId,
    subscription_id: TransportSubscriptionId,
}

impl SubscriptionAccepted {
    /// Creates a subscription acceptance response.
    #[must_use]
    pub const fn new(request_id: MessageId, subscription_id: TransportSubscriptionId) -> Self {
        Self {
            request_id,
            subscription_id,
        }
    }

    /// Returns the subscription request identifier.
    #[must_use]
    pub const fn request_id(&self) -> &MessageId {
        &self.request_id
    }

    /// Returns the assigned transport subscription identifier.
    #[must_use]
    pub const fn subscription_id(&self) -> &TransportSubscriptionId {
        &self.subscription_id
    }
}

/// Reports that a transport subscription has closed.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubscriptionClosed {
    request_id: Option<MessageId>,
    subscription_id: TransportSubscriptionId,
}

impl SubscriptionClosed {
    /// Creates a subscription closure notice.
    #[must_use]
    pub const fn new(
        request_id: Option<MessageId>,
        subscription_id: TransportSubscriptionId,
    ) -> Self {
        Self {
            request_id,
            subscription_id,
        }
    }

    /// Returns the related unsubscribe request, when client initiated.
    #[must_use]
    pub const fn request_id(&self) -> Option<&MessageId> {
        self.request_id.as_ref()
    }

    /// Returns the closed transport subscription identifier.
    #[must_use]
    pub const fn subscription_id(&self) -> &TransportSubscriptionId {
        &self.subscription_id
    }
}

/// A decoded message sent from a client into Transport Core.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum InboundMessage {
    /// Starts version and codec negotiation.
    Hello(Hello),
    /// Supplies credentials for trusted actor resolution.
    Authenticate(Authenticate),
    /// Requests an event subscription.
    Subscribe(SubscriptionRequest),
    /// Requests subscription closure.
    Unsubscribe(Unsubscribe),
    /// Carries an OrbitRelay Action data-plane envelope.
    Action(MessageEnvelope<Action>),
    /// Carries a protocol 0.2 Query request.
    Query(QueryMessage),
    /// Carries a transport heartbeat request.
    Ping(PingMessage),
    /// Requests transport connection closure.
    Close(CloseMessage),
}

/// A message emitted by Transport Core toward a client.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum OutboundMessage {
    /// Confirms negotiated protocol and codec selection.
    HelloAccepted(HelloAccepted),
    /// Confirms completed action execution.
    ActionAcknowledgement(ActionAcknowledgement),
    /// Carries a protocol 0.2 Query response.
    QueryResponse(QueryResponseMessage),
    /// Confirms creation of an event subscription.
    SubscriptionAccepted(SubscriptionAccepted),
    /// Reports subscription closure.
    SubscriptionClosed(SubscriptionClosed),
    /// Carries an OrbitRelay Event data-plane envelope.
    Event(MessageEnvelope<Event>),
    /// Carries a stable, client-safe transport error.
    Error(ErrorMessage),
    /// Carries a transport heartbeat response.
    Pong(PongMessage),
    /// Reports transport connection closure.
    Close(CloseMessage),
}
