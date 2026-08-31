//! Generic Query execution boundary.

use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    QueryActorContext, QueryFailure, QueryFailureCode, QueryHandlerError, QueryRegistry,
    QueryRequest, QueryResponse,
};

/// Transport-facing Query execution port.
#[async_trait]
pub trait QueryExecutor: Send + Sync {
    /// Executes a legal request and always returns a correlated response.
    async fn execute(&self, actor: QueryActorContext, request: QueryRequest) -> QueryResponse;
}

/// Executes registered handlers without Action/Event runtime involvement.
#[derive(Clone)]
pub struct RegisteredQueryExecutor {
    registry: Arc<QueryRegistry>,
}

impl RegisteredQueryExecutor {
    /// Creates an executor over an immutable handler registry snapshot.
    #[must_use]
    pub fn new(registry: Arc<QueryRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl QueryExecutor for RegisteredQueryExecutor {
    async fn execute(&self, actor: QueryActorContext, request: QueryRequest) -> QueryResponse {
        let request_id = request.request_id().clone();
        let query_type = request.query_type().clone();
        let Some(handler) = self.registry.get(&query_type) else {
            return QueryResponse::error(
                request_id,
                query_type,
                QueryFailure::new(
                    QueryFailureCode::UnsupportedQuery,
                    "The query type is not supported.",
                    false,
                ),
            );
        };

        match handler.execute(&actor, request).await {
            Ok(payload) => QueryResponse::success(request_id, query_type, payload),
            Err(error) => {
                let (code, retryable, message) = map_handler_error(error);
                QueryResponse::error(
                    request_id,
                    query_type,
                    QueryFailure::new(code, message, retryable),
                )
            }
        }
    }
}

fn map_handler_error(error: QueryHandlerError) -> (QueryFailureCode, bool, &'static str) {
    match error {
        QueryHandlerError::InvalidQuery => (
            QueryFailureCode::InvalidQuery,
            false,
            "The query is invalid.",
        ),
        QueryHandlerError::Unauthorized => (
            QueryFailureCode::Unauthorized,
            false,
            "The actor is not authorized to read this resource.",
        ),
        QueryHandlerError::NotFound => (
            QueryFailureCode::NotFound,
            false,
            "The requested resource was not found.",
        ),
        QueryHandlerError::Unavailable => (
            QueryFailureCode::Unavailable,
            true,
            "The query service is temporarily unavailable.",
        ),
        QueryHandlerError::Internal => (
            QueryFailureCode::Internal,
            false,
            "The query could not be completed.",
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use orbitrelay_protocol::{ActorId, MessageId, Payload};

    use super::{QueryExecutor, RegisteredQueryExecutor};
    use crate::{
        QueryActorContext, QueryHandler, QueryHandlerError, QueryRegistry, QueryRequest,
        QueryResult, QueryType,
    };

    struct Handler {
        query_type: QueryType,
        error: Option<QueryHandlerError>,
    }

    #[async_trait]
    impl QueryHandler for Handler {
        fn query_type(&self) -> &QueryType {
            &self.query_type
        }

        async fn execute(
            &self,
            _actor: &QueryActorContext,
            _request: QueryRequest,
        ) -> Result<Payload, QueryHandlerError> {
            self.error.clone().map_or_else(|| Ok(Payload::new()), Err)
        }
    }

    #[tokio::test]
    async fn registry_and_executor_preserve_identity_and_map_failures() {
        let query_type = QueryType::new("document.list").expect("valid type");
        let mut registry = QueryRegistry::new();
        registry
            .register(Arc::new(Handler {
                query_type: query_type.clone(),
                error: None,
            }))
            .expect("register");
        assert!(registry
            .register(Arc::new(Handler {
                query_type: query_type.clone(),
                error: None,
            }))
            .is_err());

        let executor = RegisteredQueryExecutor::new(Arc::new(registry));
        let request_id = MessageId::new();
        let response = executor
            .execute(
                QueryActorContext::new(ActorId::new()),
                QueryRequest::new(request_id.clone(), query_type.clone(), Payload::new()),
            )
            .await;
        assert_eq!(response.request_id(), &request_id);
        assert_eq!(response.query_type(), &query_type);
        assert!(matches!(response.result(), QueryResult::Success(_)));
    }

    #[tokio::test]
    async fn unknown_query_is_an_application_error_response() {
        let executor = RegisteredQueryExecutor::new(Arc::new(QueryRegistry::new()));
        let query_type = QueryType::new("document.get").expect("valid type");
        let response = executor
            .execute(
                QueryActorContext::new(ActorId::new()),
                QueryRequest::new(MessageId::new(), query_type, Payload::new()),
            )
            .await;
        assert!(
            matches!(response.result(), QueryResult::Error(failure) if failure.code() == crate::QueryFailureCode::UnsupportedQuery)
        );
    }

    #[tokio::test]
    async fn handler_failures_are_stable_application_errors() {
        let cases = [
            (
                QueryHandlerError::InvalidQuery,
                crate::QueryFailureCode::InvalidQuery,
                false,
            ),
            (
                QueryHandlerError::Unauthorized,
                crate::QueryFailureCode::Unauthorized,
                false,
            ),
            (
                QueryHandlerError::NotFound,
                crate::QueryFailureCode::NotFound,
                false,
            ),
            (
                QueryHandlerError::Unavailable,
                crate::QueryFailureCode::Unavailable,
                true,
            ),
            (
                QueryHandlerError::Internal,
                crate::QueryFailureCode::Internal,
                false,
            ),
        ];

        for (error, expected_code, retryable) in cases {
            let query_type = QueryType::new("document.get").expect("valid type");
            let mut registry = QueryRegistry::new();
            registry
                .register(Arc::new(Handler {
                    query_type: query_type.clone(),
                    error: Some(error),
                }))
                .expect("register");
            let executor = RegisteredQueryExecutor::new(Arc::new(registry));
            let response = executor
                .execute(
                    QueryActorContext::new(ActorId::new()),
                    QueryRequest::new(MessageId::new(), query_type, Payload::new()),
                )
                .await;
            assert!(matches!(
                response.result(),
                QueryResult::Error(failure)
                    if failure.code() == expected_code && failure.retryable() == retryable
            ));
        }
    }
}
