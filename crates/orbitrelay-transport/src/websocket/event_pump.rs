//! Independent event pump for one transport subscription.

use std::sync::Arc;

use orbitrelay_core::Version;
use orbitrelay_protocol::{MessageEnvelope, MessageId, MessageType};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    ErrorMessage, EventSource, EventSourceError, MessageCodec, OutboundMessage, TransportError,
    TransportSubscriptionId,
};

use super::coordinator::{CoordinatorEvent, WriterCommand};

/// Runs an event source until cancellation, source completion, or a fatal lag.
pub(crate) async fn run_event_pump(
    id: TransportSubscriptionId,
    mut source: Box<dyn EventSource>,
    version: Version,
    codec: Arc<dyn MessageCodec>,
    writer: mpsc::Sender<WriterCommand>,
    coordinator: mpsc::Sender<CoordinatorEvent>,
    cancellation: CancellationToken,
) {
    loop {
        let next = tokio::select! {
            _ = cancellation.cancelled() => {
                let _ = source.close().await;
                return;
            }
            result = source.next_event() => result,
        };

        match next {
            Ok(Some(event)) => {
                let message = OutboundMessage::Event(MessageEnvelope::new(
                    version,
                    MessageId::new(),
                    MessageType::new("event"),
                    event,
                ));
                if writer
                    .try_send(WriterCommand::Application {
                        message,
                        codec: Arc::clone(&codec),
                    })
                    .is_err()
                {
                    let _ = coordinator
                        .send(CoordinatorEvent::SlowConsumer { id: id.clone() })
                        .await;
                    let _ = source.close().await;
                    return;
                }
            }
            Ok(None) => {
                let _ = coordinator
                    .send(CoordinatorEvent::SubscriptionFinished { id: id.clone() })
                    .await;
                return;
            }
            Err(error) => {
                let transport_error = TransportError::EventSource(error);
                let message = OutboundMessage::Error(ErrorMessage::from_transport_error(
                    None,
                    &transport_error,
                ));
                let error_queued = writer
                    .try_send(WriterCommand::Application {
                        message,
                        codec: Arc::clone(&codec),
                    })
                    .is_ok();
                let fatal = matches!(
                    &transport_error,
                    TransportError::EventSource(EventSourceError::SubscriptionLagged)
                );
                let _ = coordinator
                    .send(CoordinatorEvent::EventSourceFailed {
                        id: id.clone(),
                        error: transport_error,
                        fatal,
                        error_queued,
                    })
                    .await;
                let _ = source.close().await;
                return;
            }
        }
    }
}
