//! Per-connection sequential Action execution worker.

use std::sync::Arc;

use orbitrelay_protocol::{Action, MessageId};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::ActionExecutor;

use super::coordinator::CoordinatorEvent;

/// Work item submitted after coordinator identity and state checks.
pub(crate) struct ActionJob {
    /// Original envelope message id used for acknowledgement correlation.
    pub request_id: MessageId,
    /// Action passed to the execution port.
    pub action: Action,
}

/// Executes one connection's actions in arrival order.
pub(crate) async fn run_action_worker(
    mut receiver: mpsc::Receiver<ActionJob>,
    executor: Arc<dyn ActionExecutor>,
    events: mpsc::Sender<CoordinatorEvent>,
    cancellation: CancellationToken,
) {
    loop {
        let job = tokio::select! {
            _ = cancellation.cancelled() => return,
            job = receiver.recv() => job,
        };
        let Some(job) = job else {
            return;
        };

        let request_id = job.request_id;
        let action_id = job.action.id().clone();
        let action = job.action;
        let result = tokio::select! {
            _ = cancellation.cancelled() => return,
            result = executor.execute(action) => result,
        };
        if events
            .send(CoordinatorEvent::ActionCompleted {
                request_id,
                action_id,
                result,
            })
            .await
            .is_err()
        {
            return;
        }
    }
}
