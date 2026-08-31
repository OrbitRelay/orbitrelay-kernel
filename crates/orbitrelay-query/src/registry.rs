//! Query handler registration.

use std::collections::HashMap;
use std::sync::Arc;

use thiserror::Error;

use crate::{QueryHandler, QueryType};

/// Registration failure for a Query handler.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum QueryRegistryError {
    /// A handler for the Query type is already registered.
    #[error("query handler already registered for `{query_type}`")]
    DuplicateQueryType {
        /// The duplicated Query type.
        query_type: QueryType,
    },
}

/// A thread-safe registry of typed Query handlers.
#[derive(Clone, Default)]
pub struct QueryRegistry {
    handlers: HashMap<QueryType, Arc<dyn QueryHandler>>,
}

impl QueryRegistry {
    /// Creates an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers one handler, rejecting duplicate Query types.
    pub fn register(&mut self, handler: Arc<dyn QueryHandler>) -> Result<(), QueryRegistryError> {
        let query_type = handler.query_type().clone();
        if self.handlers.contains_key(&query_type) {
            return Err(QueryRegistryError::DuplicateQueryType { query_type });
        }
        self.handlers.insert(query_type, handler);
        Ok(())
    }

    /// Looks up a handler for a Query type.
    #[must_use]
    pub fn get(&self, query_type: &QueryType) -> Option<Arc<dyn QueryHandler>> {
        self.handlers.get(query_type).cloned()
    }
}
