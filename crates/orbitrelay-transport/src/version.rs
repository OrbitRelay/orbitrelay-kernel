//! Connection-level protocol version negotiation.

use orbitrelay_core::Version;

use crate::VersionNegotiationError;

/// The first exact protocol version supported by Transport Core.
pub const CURRENT_PROTOCOL_VERSION: Version = Version::new(0, 1, 0);

/// Protocol version that adds the generic Query read plane.
pub const QUERY_PROTOCOL_VERSION: Version = Version::new(0, 2, 0);

/// Selects a protocol version from versions advertised by a client.
pub trait VersionPolicy: Send + Sync {
    /// Returns the selected version or reports that no version is compatible.
    fn negotiate(&self, supported_versions: &[Version])
        -> Result<Version, VersionNegotiationError>;
}

/// A policy accepting only [`CURRENT_PROTOCOL_VERSION`].
#[derive(Clone, Copy, Debug, Default)]
pub struct ExactVersionPolicy;

impl VersionPolicy for ExactVersionPolicy {
    fn negotiate(
        &self,
        supported_versions: &[Version],
    ) -> Result<Version, VersionNegotiationError> {
        if supported_versions.contains(&CURRENT_PROTOCOL_VERSION) {
            Ok(CURRENT_PROTOCOL_VERSION)
        } else {
            Err(VersionNegotiationError::UnsupportedVersion {
                supported_versions: supported_versions.to_vec(),
            })
        }
    }
}

/// A policy accepting only the Query-capable protocol version.
#[derive(Clone, Copy, Debug, Default)]
pub struct ExactQueryVersionPolicy;

impl VersionPolicy for ExactQueryVersionPolicy {
    fn negotiate(
        &self,
        supported_versions: &[Version],
    ) -> Result<Version, VersionNegotiationError> {
        if supported_versions.contains(&QUERY_PROTOCOL_VERSION) {
            Ok(QUERY_PROTOCOL_VERSION)
        } else {
            Err(VersionNegotiationError::UnsupportedVersion {
                supported_versions: supported_versions.to_vec(),
            })
        }
    }
}

/// A development policy accepting both the frozen 0.1 and Query-capable 0.2.
///
/// The highest mutually supported version is selected, so a client that
/// advertises both receives 0.2 while an existing 0.1 client remains valid.
#[derive(Clone, Copy, Debug, Default)]
pub struct CompatibleVersionPolicy;

impl VersionPolicy for CompatibleVersionPolicy {
    fn negotiate(
        &self,
        supported_versions: &[Version],
    ) -> Result<Version, VersionNegotiationError> {
        if supported_versions.contains(&QUERY_PROTOCOL_VERSION) {
            Ok(QUERY_PROTOCOL_VERSION)
        } else if supported_versions.contains(&CURRENT_PROTOCOL_VERSION) {
            Ok(CURRENT_PROTOCOL_VERSION)
        } else {
            Err(VersionNegotiationError::UnsupportedVersion {
                supported_versions: supported_versions.to_vec(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use orbitrelay_core::Version;

    use super::{ExactVersionPolicy, VersionPolicy, CURRENT_PROTOCOL_VERSION};

    #[test]
    fn exact_policy_selects_current_version() {
        let selected = ExactVersionPolicy
            .negotiate(&[Version::new(1, 0, 0), CURRENT_PROTOCOL_VERSION])
            .expect("current version should be compatible");

        assert_eq!(selected, CURRENT_PROTOCOL_VERSION);
    }

    #[test]
    fn exact_policy_rejects_other_versions() {
        let result = ExactVersionPolicy.negotiate(&[Version::new(0, 2, 0)]);

        assert!(result.is_err());
    }
}
