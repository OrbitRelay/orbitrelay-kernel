//! Query handler contract and stable handler failures.

use async_trait::async_trait;
use orbitrelay_protocol::Payload;
use thiserror::Error;

use crate::{QueryActorContext, QueryRequest};

/// A domain-neutral asynchronous Query handler.
#[async_trait]
pub trait QueryHandler: Send + Sync {
    /// Returns the single Query type handled by this value.
    fn query_type(&self) -> &crate::QueryType;

    /// Executes a validated request for a trusted actor context.
    async fn execute(
        &self,
        actor: &QueryActorContext,
        request: QueryRequest,
    ) -> Result<Payload, QueryHandlerError>;
}

/// Stable categories a typed Query handler can return.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum QueryHandlerError {
    /// The typed request payload is invalid.
    #[error("invalid query payload")]
    InvalidQuery,
    /// The actor is not permitted to read the resolved target.
    #[error("query is unauthorized")]
    Unauthorized,
    /// The requested resource does not exist.
    #[error("query resource was not found")]
    NotFound,
    /// The read backend is unavailable.
    #[error("query backend is unavailable")]
    Unavailable,
    /// An unexpected handler failure occurred.
    #[error("query handler failed")]
    Internal,
}
