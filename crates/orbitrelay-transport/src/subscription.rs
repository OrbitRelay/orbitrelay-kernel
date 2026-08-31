//! Transport-visible subscription requests and authorization boundary.

use std::{collections::BTreeSet, fmt, str::FromStr};

use async_trait::async_trait;
use orbitrelay_core::{CoreError, EntityId};
use orbitrelay_protocol::{EventType, MessageId, SessionId};
use serde::{Deserialize, Serialize};

use crate::{ActorBinding, SubscriptionAuthorizationError};

/// Identifies one transport subscription without exposing a sync identifier.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TransportSubscriptionId(EntityId);

impl TransportSubscriptionId {
    /// Creates a new random transport subscription identifier.
    #[must_use]
    #[allow(
        clippy::new_without_default,
        reason = "creating a subscription identity must remain an explicit operation"
    )]
    pub fn new() -> Self {
        Self(EntityId::new())
    }

    /// Wraps an existing core entity identifier.
    #[must_use]
    pub const fn from_entity_id(value: EntityId) -> Self {
        Self(value)
    }

    /// Returns the wrapped core entity identifier.
    #[must_use]
    pub const fn as_entity_id(&self) -> &EntityId {
        &self.0
    }

    /// Parses an identifier from a UUID string.
    pub fn parse(value: &str) -> Result<Self, CoreError> {
        Ok(Self(EntityId::parse(value)?))
    }
}

impl FromStr for TransportSubscriptionId {
    type Err = CoreError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl fmt::Display for TransportSubscriptionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// A request to receive selected event types from one session.
///
/// An empty event type set subscribes to every event type in the session.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubscriptionRequest {
    request_id: MessageId,
    session_id: SessionId,
    event_types: BTreeSet<EventType>,
}

impl SubscriptionRequest {
    /// Creates a session subscription with a stable, deduplicated event filter.
    #[must_use]
    pub fn new(
        request_id: MessageId,
        session_id: SessionId,
        event_types: impl IntoIterator<Item = EventType>,
    ) -> Self {
        Self {
            request_id,
            session_id,
            event_types: event_types.into_iter().collect(),
        }
    }

    /// Returns the control request identifier.
    #[must_use]
    pub const fn request_id(&self) -> &MessageId {
        &self.request_id
    }

    /// Returns the session to subscribe to.
    #[must_use]
    pub const fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    /// Returns the selected event types in stable order.
    #[must_use]
    pub const fn event_types(&self) -> &BTreeSet<EventType> {
        &self.event_types
    }

    /// Returns whether all event types are selected.
    #[must_use]
    pub fn selects_all_event_types(&self) -> bool {
        self.event_types.is_empty()
    }
}

/// Authorizes a trusted actor before an event source is created.
#[async_trait]
pub trait SubscriptionAuthorizer: Send + Sync {
    /// Allows or rejects a subscription request for the bound actor.
    async fn authorize(
        &self,
        binding: &ActorBinding,
        request: &SubscriptionRequest,
    ) -> Result<(), SubscriptionAuthorizationError>;
}

#[cfg(test)]
mod tests {
    use orbitrelay_protocol::{EventType, MessageId, SessionId};

    use super::SubscriptionRequest;

    #[test]
    fn subscription_request_deduplicates_and_orders_event_types() {
        let request = SubscriptionRequest::new(
            MessageId::new(),
            SessionId::new(),
            [
                EventType::new("z.event"),
                EventType::new("a.event"),
                EventType::new("z.event"),
            ],
        );
        let values = request
            .event_types()
            .iter()
            .map(EventType::as_str)
            .collect::<Vec<_>>();

        assert_eq!(values, vec!["a.event", "z.event"]);
    }

    #[test]
    fn empty_event_types_select_all_events() {
        let request =
            SubscriptionRequest::new(MessageId::new(), SessionId::new(), std::iter::empty());

        assert!(request.selects_all_event_types());
    }
}
