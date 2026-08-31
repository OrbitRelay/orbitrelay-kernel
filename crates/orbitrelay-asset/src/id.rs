//! Strong Asset identifiers.

use std::{fmt, str::FromStr};

use orbitrelay_core::{CoreError, EntityId};
use serde::{Deserialize, Serialize};

/// Identifies one immutable source Asset independently of its bytes or
/// storage backend.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AssetId(EntityId);

impl AssetId {
    /// Creates a new random UUID v4 Asset identifier.
    #[must_use]
    #[allow(
        clippy::new_without_default,
        reason = "creating a domain identity must remain an explicit operation"
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

    /// Parses a UUID string into an Asset identifier.
    pub fn parse(value: &str) -> Result<Self, CoreError> {
        Ok(Self(EntityId::parse(value)?))
    }
}

impl FromStr for AssetId {
    type Err = CoreError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl fmt::Display for AssetId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[cfg(test)]
mod tests {
    use std::any::TypeId;

    use super::AssetId;

    #[test]
    fn asset_id_is_a_distinct_strong_type() {
        assert_ne!(
            TypeId::of::<AssetId>(),
            TypeId::of::<orbitrelay_core::EntityId>()
        );
    }

    #[test]
    fn asset_id_round_trips_through_json_and_strings() {
        let asset_id = AssetId::new();
        let encoded = serde_json::to_string(&asset_id).expect("AssetId should serialize");
        let decoded: AssetId = serde_json::from_str(&encoded).expect("AssetId should deserialize");

        assert_eq!(decoded, asset_id);
        assert_eq!(
            asset_id
                .to_string()
                .parse::<AssetId>()
                .expect("AssetId should parse"),
            asset_id
        );
    }
}
