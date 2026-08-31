//! Trusted connection identity resolution and action binding checks.

use std::fmt;

use async_trait::async_trait;
use orbitrelay_protocol::{Action, ActorId};
use serde::{Deserialize, Serialize};

use crate::{ConnectionId, IdentityError};

/// An extensible identifier for the system that resolved an identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IdentitySource(String);

impl IdentitySource {
    /// Creates an identity source such as `gateway` or `local_development`.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the identity source string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for IdentitySource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// A trusted association between a connection and a protocol actor.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorBinding {
    actor_id: ActorId,
    source: IdentitySource,
}

impl ActorBinding {
    /// Creates a trusted actor binding returned by an identity resolver.
    #[must_use]
    pub const fn new(actor_id: ActorId, source: IdentitySource) -> Self {
        Self { actor_id, source }
    }

    /// Returns the trusted actor identifier.
    #[must_use]
    pub const fn actor_id(&self) -> &ActorId {
        &self.actor_id
    }

    /// Returns the identity resolution source.
    #[must_use]
    pub const fn source(&self) -> &IdentitySource {
        &self.source
    }
}

/// Opaque credentials supplied to a transport identity resolver.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InboundCredentials {
    scheme: String,
    credential: String,
}

impl InboundCredentials {
    /// Creates credentials for a resolver-defined authentication scheme.
    #[must_use]
    pub fn new(scheme: impl Into<String>, credential: impl Into<String>) -> Self {
        Self {
            scheme: scheme.into(),
            credential: credential.into(),
        }
    }

    /// Returns the resolver-defined credential scheme.
    #[must_use]
    pub fn scheme(&self) -> &str {
        &self.scheme
    }

    /// Returns the credential secret for trusted resolver implementations.
    #[must_use]
    pub fn credential(&self) -> &str {
        &self.credential
    }
}

impl fmt::Debug for InboundCredentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("InboundCredentials")
            .field("scheme", &self.scheme)
            .field("credential", &"[REDACTED]")
            .finish()
    }
}

/// Resolves transport credentials into a trusted actor binding.
#[async_trait]
pub trait IdentityResolver: Send + Sync {
    /// Resolves credentials for one connection.
    async fn resolve(
        &self,
        connection_id: &ConnectionId,
        credentials: &InboundCredentials,
    ) -> Result<ActorBinding, IdentityError>;
}

/// Verifies that an action originates from the actor trusted for a connection.
///
/// The function rejects mismatches and never rewrites the action actor.
pub fn validate_action_binding(
    binding: Option<&ActorBinding>,
    action: &Action,
) -> Result<(), IdentityError> {
    let binding = binding.ok_or(IdentityError::AuthenticationRequired)?;
    if binding.actor_id() != action.actor_id() {
        return Err(IdentityError::IdentityMismatch {
            bound_actor_id: binding.actor_id().clone(),
            action_actor_id: action.actor_id().clone(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use orbitrelay_core::{Metadata, Timestamp};
    use orbitrelay_protocol::{Action, ActionId, ActionType, ActorId, Payload, SessionId};

    use super::{validate_action_binding, ActorBinding, IdentitySource, InboundCredentials};
    use crate::IdentityError;

    fn action(actor_id: ActorId) -> Action {
        Action::new(
            ActionId::new(),
            SessionId::new(),
            actor_id,
            ActionType::new("canvas.draw"),
            Timestamp::from_unix_timestamp(1_700_000_000).expect("valid timestamp"),
            Payload::new(),
            Metadata::new(),
        )
    }

    #[test]
    fn creates_actor_binding_for_any_actor_kind() {
        let actor_id = ActorId::new();
        let binding = ActorBinding::new(actor_id.clone(), IdentitySource::new("gateway"));

        assert_eq!(binding.actor_id(), &actor_id);
        assert_eq!(binding.source().as_str(), "gateway");
    }

    #[test]
    fn credentials_debug_output_is_redacted() {
        let credentials = InboundCredentials::new("bearer", "secret-value");
        let debug = format!("{credentials:?}");

        assert!(!debug.contains("secret-value"));
        assert!(debug.contains("[REDACTED]"));
    }

    #[test]
    fn rejects_action_without_authentication() {
        let result = validate_action_binding(None, &action(ActorId::new()));

        assert_eq!(result, Err(IdentityError::AuthenticationRequired));
    }

    #[test]
    fn rejects_action_from_a_different_actor() {
        let binding = ActorBinding::new(ActorId::new(), IdentitySource::new("test"));
        let result = validate_action_binding(Some(&binding), &action(ActorId::new()));

        assert!(matches!(
            result,
            Err(IdentityError::IdentityMismatch { .. })
        ));
    }

    #[test]
    fn accepts_action_from_bound_actor() {
        let actor_id = ActorId::new();
        let binding = ActorBinding::new(actor_id.clone(), IdentitySource::new("test"));

        assert!(validate_action_binding(Some(&binding), &action(actor_id)).is_ok());
    }
}
