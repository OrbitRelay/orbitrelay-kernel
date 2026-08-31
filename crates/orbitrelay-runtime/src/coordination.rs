//! Domain-neutral coordination boundaries for aggregate action execution.

use async_trait::async_trait;

use crate::ExecutionCoordinationError;

/// A server-side key identifying actions that must execute serially.
///
/// Scopes are created by handlers after action validation. They are runtime
/// coordination values and are never accepted directly from protocol clients.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExecutionScope {
    namespace: String,
    key: String,
}

impl ExecutionScope {
    /// Creates a scope from a domain namespace and aggregate key.
    ///
    /// Empty and whitespace-only values are rejected. Values are otherwise
    /// preserved without normalization so handlers control canonicalization.
    pub fn new(
        namespace: impl Into<String>,
        key: impl Into<String>,
    ) -> Result<Self, ExecutionCoordinationError> {
        let namespace = namespace.into();
        if namespace.trim().is_empty() {
            return Err(ExecutionCoordinationError::InvalidScope { field: "namespace" });
        }

        let key = key.into();
        if key.trim().is_empty() {
            return Err(ExecutionCoordinationError::InvalidScope { field: "key" });
        }

        Ok(Self { namespace, key })
    }

    /// Returns the domain namespace of this scope.
    #[must_use]
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// Returns the canonical aggregate key within the namespace.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }
}

/// A held right to execute within one [`ExecutionScope`].
///
/// The lease has no release method. Dropping it must release the execution
/// right, including when action handling or event dispatch returns an error.
pub trait ExecutionLease: Send {}

/// Acquires execution leases for domain-defined scopes.
///
/// Implementations define the coordination mechanism and must guarantee that
/// equal scopes are mutually exclusive while allowing different scopes to
/// proceed independently. If an acquisition future is cancelled, it must not
/// retain a waiter, lock, or other coordination resource indefinitely.
#[async_trait]
pub trait ExecutionCoordinator: Send + Sync {
    /// Acquires an RAII lease for `scope`.
    async fn acquire(
        &self,
        scope: &ExecutionScope,
    ) -> Result<Box<dyn ExecutionLease>, ExecutionCoordinationError>;
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashSet};

    use super::ExecutionScope;
    use crate::ExecutionCoordinationError;

    #[test]
    fn execution_scope_preserves_valid_values_and_supports_key_traits() {
        let scope =
            ExecutionScope::new("document", "document-1").expect("non-empty scope should be valid");

        assert_eq!(scope.namespace(), "document");
        assert_eq!(scope.key(), "document-1");
        assert!(HashSet::from([scope.clone()]).contains(&scope));
        assert!(BTreeSet::from([scope.clone()]).contains(&scope));
        assert_eq!(scope, scope.clone());
    }

    #[test]
    fn execution_scope_rejects_empty_or_whitespace_only_values() {
        assert!(matches!(
            ExecutionScope::new("", "key"),
            Err(ExecutionCoordinationError::InvalidScope { field: "namespace" })
        ));
        assert!(matches!(
            ExecutionScope::new("namespace", " \t"),
            Err(ExecutionCoordinationError::InvalidScope { field: "key" })
        ));
    }
}
