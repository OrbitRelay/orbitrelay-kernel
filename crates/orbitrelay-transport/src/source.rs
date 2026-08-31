//! Event source ports consumed by transport event pumps.

use async_trait::async_trait;
use orbitrelay_protocol::Event;

use crate::{EventSourceError, SubscriptionRequest, TransportSubscriptionId};

/// A transport-owned stream of subscribed events.
#[async_trait]
pub trait EventSource: Send {
    /// Returns the transport-visible subscription identifier.
    fn id(&self) -> &TransportSubscriptionId;

    /// Waits for the next event, returning `None` after normal completion.
    async fn next_event(&mut self) -> Result<Option<Event>, EventSourceError>;

    /// Closes the event source and releases its subscription resources.
    async fn close(&mut self) -> Result<(), EventSourceError>;
}

/// Creates event sources without exposing an EventBus implementation.
#[async_trait]
pub trait EventSourceFactory: Send + Sync {
    /// Creates an event source for an already-authorized request.
    async fn subscribe(
        &self,
        request: SubscriptionRequest,
    ) -> Result<Box<dyn EventSource>, EventSourceError>;
}
