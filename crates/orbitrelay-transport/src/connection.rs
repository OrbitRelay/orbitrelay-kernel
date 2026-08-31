//! Transport-independent connection identity and lifecycle state.

use std::{fmt, str::FromStr};

use orbitrelay_core::{CoreError, EntityId, Metadata};
use serde::{Deserialize, Serialize};

use crate::{ActorBinding, ConnectionStateError, IdentityError, TransportError};

/// Identifies one concrete external connection.
///
/// A connection identifier is deliberately incompatible with an actor
/// identifier:
///
/// ```compile_fail
/// use orbitrelay_protocol::ActorId;
/// use orbitrelay_transport::ConnectionId;
///
/// fn accept_connection(_: ConnectionId) {}
/// accept_connection(ActorId::new());
/// ```
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConnectionId(EntityId);

impl ConnectionId {
    /// Creates a new random connection identifier.
    #[must_use]
    #[allow(
        clippy::new_without_default,
        reason = "creating a connection identity must remain an explicit operation"
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

    /// Parses a connection identifier from a UUID string.
    pub fn parse(value: &str) -> Result<Self, CoreError> {
        Ok(Self(EntityId::parse(value)?))
    }
}

impl FromStr for ConnectionId {
    type Err = CoreError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl fmt::Display for ConnectionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Generic metadata describing a connection without binding network details.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConnectionMetadata(Metadata);

impl ConnectionMetadata {
    /// Creates empty connection metadata.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Wraps generic core metadata.
    #[must_use]
    pub const fn from_metadata(metadata: Metadata) -> Self {
        Self(metadata)
    }

    /// Returns the wrapped metadata.
    #[must_use]
    pub const fn as_metadata(&self) -> &Metadata {
        &self.0
    }

    /// Returns the number of metadata entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether the metadata is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Inserts a metadata entry.
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) -> Option<String> {
        self.0.insert(key, value)
    }

    /// Returns a metadata value by key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key)
    }

    /// Removes a metadata entry.
    pub fn remove(&mut self, key: &str) -> Option<String> {
        self.0.remove(key)
    }

    /// Iterates over metadata entries in key order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0.iter()
    }
}

/// Lifecycle state of a transport-independent connection.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    /// Protocol version and codec negotiation is in progress.
    Negotiating,
    /// Trusted actor identity resolution is in progress.
    Authenticating,
    /// The connection can exchange data-plane messages.
    Ready,
    /// Shutdown has started and new work must be rejected.
    Closing,
    /// The connection is fully closed.
    Closed,
}

impl ConnectionState {
    /// Returns whether this state may transition to `next`.
    #[must_use]
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Negotiating, Self::Authenticating)
                | (Self::Authenticating, Self::Ready)
                | (Self::Ready, Self::Closing)
                | (Self::Negotiating, Self::Closing)
                | (Self::Authenticating, Self::Closing)
                | (Self::Closing, Self::Closed)
        )
    }

    /// Applies a legal state transition.
    pub fn transition_to(&mut self, next: Self) -> Result<(), ConnectionStateError> {
        if !self.can_transition_to(next) {
            return Err(ConnectionStateError::InvalidTransition {
                from: *self,
                to: next,
            });
        }
        *self = next;
        Ok(())
    }
}

/// Connection-local lifecycle and trusted actor binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransportConnection {
    id: ConnectionId,
    metadata: ConnectionMetadata,
    state: ConnectionState,
    identity: Option<ActorBinding>,
}

impl TransportConnection {
    /// Creates a connection in the negotiation state.
    #[must_use]
    pub const fn new(id: ConnectionId, metadata: ConnectionMetadata) -> Self {
        Self {
            id,
            metadata,
            state: ConnectionState::Negotiating,
            identity: None,
        }
    }

    /// Returns the connection identifier.
    #[must_use]
    pub const fn id(&self) -> &ConnectionId {
        &self.id
    }

    /// Returns connection metadata.
    #[must_use]
    pub const fn metadata(&self) -> &ConnectionMetadata {
        &self.metadata
    }

    /// Returns the current lifecycle state.
    #[must_use]
    pub const fn state(&self) -> ConnectionState {
        self.state
    }

    /// Returns the trusted actor binding, when authentication completed.
    #[must_use]
    pub const fn identity(&self) -> Option<&ActorBinding> {
        self.identity.as_ref()
    }

    /// Moves from negotiation into identity resolution.
    pub fn begin_authentication(&mut self) -> Result<(), ConnectionStateError> {
        self.state.transition_to(ConnectionState::Authenticating)
    }

    /// Stores a trusted binding while the connection is authenticating.
    pub fn bind_identity(&mut self, identity: ActorBinding) -> Result<(), ConnectionStateError> {
        if self.state != ConnectionState::Authenticating {
            return Err(ConnectionStateError::OperationNotAllowed {
                operation: "bind_identity",
                state: self.state,
            });
        }
        self.identity = Some(identity);
        Ok(())
    }

    /// Marks an authenticated connection ready for data-plane messages.
    pub fn mark_ready(&mut self) -> Result<(), TransportError> {
        if self.identity.is_none() {
            return Err(IdentityError::AuthenticationRequired.into());
        }
        self.state.transition_to(ConnectionState::Ready)?;
        Ok(())
    }

    /// Starts connection shutdown from any active initialization or ready state.
    pub fn begin_close(&mut self) -> Result<(), ConnectionStateError> {
        self.state.transition_to(ConnectionState::Closing)
    }

    /// Completes connection shutdown.
    pub fn close(&mut self) -> Result<(), ConnectionStateError> {
        self.state.transition_to(ConnectionState::Closed)
    }
}

#[cfg(test)]
mod tests {
    use std::any::TypeId;

    use orbitrelay_protocol::{ActorId, SessionId};

    use super::{ConnectionId, ConnectionMetadata, ConnectionState, TransportConnection};
    use crate::{ActorBinding, IdentitySource};

    #[test]
    fn connection_id_is_a_distinct_domain_type() {
        assert_ne!(TypeId::of::<ConnectionId>(), TypeId::of::<ActorId>());
        assert_ne!(TypeId::of::<ConnectionId>(), TypeId::of::<SessionId>());
    }

    #[test]
    fn follows_the_authenticated_lifecycle() {
        let actor_id = ActorId::new();
        let mut connection =
            TransportConnection::new(ConnectionId::new(), ConnectionMetadata::new());

        connection
            .begin_authentication()
            .expect("negotiation can begin authentication");
        connection
            .bind_identity(ActorBinding::new(
                actor_id.clone(),
                IdentitySource::new("test"),
            ))
            .expect("identity can be bound while authenticating");
        connection.mark_ready().expect("bound connection is ready");
        connection
            .begin_close()
            .expect("ready connection can close");
        connection.close().expect("closing connection can close");

        assert_eq!(connection.state(), ConnectionState::Closed);
        assert_eq!(
            connection.identity().map(ActorBinding::actor_id),
            Some(&actor_id)
        );
    }

    #[test]
    fn permits_closing_during_initialization() {
        let mut connection =
            TransportConnection::new(ConnectionId::new(), ConnectionMetadata::new());

        connection
            .begin_close()
            .expect("negotiating connection can close");
        connection.close().expect("closing connection can close");

        assert_eq!(connection.state(), ConnectionState::Closed);
    }

    #[test]
    fn rejects_illegal_state_transition() {
        let mut state = ConnectionState::Negotiating;

        assert!(state.transition_to(ConnectionState::Ready).is_err());
        assert_eq!(state, ConnectionState::Negotiating);
    }

    #[test]
    fn requires_identity_before_ready() {
        let mut connection =
            TransportConnection::new(ConnectionId::new(), ConnectionMetadata::new());
        connection
            .begin_authentication()
            .expect("negotiation can begin authentication");

        assert!(connection.mark_ready().is_err());
        assert_eq!(connection.state(), ConnectionState::Authenticating);
    }
}
