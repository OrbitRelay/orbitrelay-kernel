//! Bounded concurrent Query execution for one connection.

use std::sync::Arc;

use orbitrelay_query::{QueryActorContext, QueryExecutor, QueryRequest};
use tokio::{sync::mpsc, task::JoinSet};
use tokio_util::sync::CancellationToken;

use super::coordinator::CoordinatorEvent;

/// Work submitted after connection state and protocol-version checks.
pub(crate) struct QueryJob {
    /// Trusted actor context from the authenticated connection.
    pub actor: QueryActorContext,
    /// Generic Query request.
    pub request: QueryRequest,
}

/// Executes at most `max_in_flight` Queries at once while keeping the input
/// queue bounded. Every child observes connection cancellation.
pub(crate) async fn run_query_worker(
    mut receiver: mpsc::Receiver<QueryJob>,
    executor: Arc<dyn QueryExecutor>,
    events: mpsc::Sender<CoordinatorEvent>,
    cancellation: CancellationToken,
    max_in_flight: usize,
) {
    let mut tasks = JoinSet::new();

    loop {
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => {
                tasks.abort_all();
                while tasks.join_next().await.is_some() {}
                return;
            }
            joined = tasks.join_next(), if !tasks.is_empty() => {
                if let Some(Ok(Some(response))) = joined {
                    if events.send(CoordinatorEvent::QueryCompleted { response }).await.is_err() {
                        tasks.abort_all();
                        return;
                    }
                }
            }
            job = receiver.recv(), if tasks.len() < max_in_flight => {
                let Some(job) = job else {
                    tasks.abort_all();
                    while tasks.join_next().await.is_some() {}
                    return;
                };
                let child_cancellation = cancellation.child_token();
                let executor = Arc::clone(&executor);
                tasks.spawn(async move {
                    tokio::select! {
                        _ = child_cancellation.cancelled() => None,
                        response = executor.execute(job.actor, job.request) => Some(response),
                    }
                });
            }
        }
    }
}
