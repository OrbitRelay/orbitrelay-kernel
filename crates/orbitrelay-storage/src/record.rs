//! Stored event records and cursor-based query pages.

use std::fmt;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use orbitrelay_core::EntityId;
use orbitrelay_protocol::Event;
use serde::{Deserialize, Serialize};

use crate::StorageError;

/// An opaque, backend-owned position in append order.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EventCursor(String);

impl fmt::Debug for EventCursor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("EventCursor(..)")
    }
}

impl EventCursor {
    /// Creates an opaque cursor for a non-memory storage adapter.
    ///
    /// The adapter owns the store and continuation identities. Callers should
    /// treat the returned value as an opaque token and never inspect or modify
    /// its encoded position.
    #[doc(hidden)]
    pub fn for_storage(store_id: &EntityId, continuation_epoch: &EntityId, position: u64) -> Self {
        Self(encode_storage_token(
            TokenKind::Cursor,
            store_id,
            continuation_epoch,
            position,
        ))
    }

    /// Decodes a cursor owned by a storage adapter.
    #[doc(hidden)]
    pub fn storage_position(
        &self,
        store_id: &EntityId,
        continuation_epoch: &EntityId,
    ) -> Result<u64, StorageError> {
        decode_storage_token(&self.0, TokenKind::Cursor, store_id, continuation_epoch).ok_or_else(
            || StorageError::InvalidCursor {
                reason: "cursor does not belong to this store epoch".to_owned(),
            },
        )
    }

    pub(crate) fn for_memory(store_id: &EntityId, position: usize) -> Self {
        Self(encode_memory_token(TokenKind::Cursor, store_id, position))
    }

    pub(crate) fn memory_position(
        &self,
        store_id: &EntityId,
        record_count: usize,
    ) -> Result<usize, StorageError> {
        let position = decode_memory_token(&self.0, TokenKind::Cursor, store_id)
            .and_then(|position| usize::try_from(position).ok())
            .filter(|position| *position > 0 && *position <= record_count)
            .ok_or_else(|| StorageError::InvalidCursor {
                reason: "cursor position is outside the stored event range".to_owned(),
            })?;

        Ok(position)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn from_test_token(token: impl Into<String>) -> Self {
        Self(token.into())
    }
}

/// A stable, store-owned exclusive upper bound for a replay query.
///
/// Checkpoints and [`EventCursor`] values intentionally have separate public
/// types: a cursor says where the next page starts, while a checkpoint freezes
/// where a replay must stop.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EventStoreCheckpoint(String);

impl fmt::Debug for EventStoreCheckpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("EventStoreCheckpoint(..)")
    }
}

impl EventStoreCheckpoint {
    /// Creates an opaque checkpoint for a non-memory storage adapter.
    ///
    /// The position is an exclusive append boundary owned by the adapter.
    #[doc(hidden)]
    pub fn for_storage(store_id: &EntityId, continuation_epoch: &EntityId, position: u64) -> Self {
        Self(encode_storage_token(
            TokenKind::Checkpoint,
            store_id,
            continuation_epoch,
            position,
        ))
    }

    /// Decodes a checkpoint owned by a storage adapter.
    #[doc(hidden)]
    pub fn storage_position(
        &self,
        store_id: &EntityId,
        continuation_epoch: &EntityId,
    ) -> Result<u64, StorageError> {
        decode_storage_token(&self.0, TokenKind::Checkpoint, store_id, continuation_epoch)
            .ok_or_else(|| StorageError::InvalidCheckpoint {
                reason: "checkpoint does not belong to this store epoch".to_owned(),
            })
    }

    pub(crate) fn for_memory(store_id: &EntityId, position: usize) -> Self {
        Self(encode_memory_token(
            TokenKind::Checkpoint,
            store_id,
            position,
        ))
    }

    pub(crate) fn memory_position(
        &self,
        store_id: &EntityId,
        record_count: usize,
    ) -> Result<usize, StorageError> {
        decode_memory_token(&self.0, TokenKind::Checkpoint, store_id)
            .and_then(|position| usize::try_from(position).ok())
            .filter(|position| *position <= record_count)
            .ok_or_else(|| StorageError::InvalidCheckpoint {
                reason: "checkpoint does not belong to this store or is outside its range"
                    .to_owned(),
            })
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

const TOKEN_VERSION: u8 = 1;
const TOKEN_LENGTH: usize = 26;

#[derive(Clone, Copy, Eq, PartialEq)]
#[repr(u8)]
enum TokenKind {
    Cursor = 1,
    Checkpoint = 2,
}

fn encode_memory_token(kind: TokenKind, store_id: &EntityId, position: usize) -> String {
    let position = u64::try_from(position).expect("memory append position must fit in u64");
    let mut bytes = [0_u8; TOKEN_LENGTH];
    bytes[0] = TOKEN_VERSION;
    bytes[1] = kind as u8;
    bytes[2..18].copy_from_slice(store_id.as_uuid().as_bytes());
    bytes[18..].copy_from_slice(&position.to_be_bytes());
    URL_SAFE_NO_PAD.encode(bytes)
}

fn decode_memory_token(token: &str, expected_kind: TokenKind, store_id: &EntityId) -> Option<u64> {
    let bytes = URL_SAFE_NO_PAD.decode(token).ok()?;
    if bytes.len() != TOKEN_LENGTH
        || bytes[0] != TOKEN_VERSION
        || bytes[1] != expected_kind as u8
        || bytes[2..18] != store_id.as_uuid().as_bytes()[..]
    {
        return None;
    }
    let position = <[u8; 8]>::try_from(&bytes[18..]).ok()?;
    Some(u64::from_be_bytes(position))
}

const STORAGE_TOKEN_VERSION: u8 = 1;
const STORAGE_TOKEN_LENGTH: usize = 42;

fn encode_storage_token(
    kind: TokenKind,
    store_id: &EntityId,
    continuation_epoch: &EntityId,
    position: u64,
) -> String {
    let mut bytes = [0_u8; STORAGE_TOKEN_LENGTH];
    bytes[0] = STORAGE_TOKEN_VERSION;
    bytes[1] = kind as u8;
    bytes[2..18].copy_from_slice(store_id.as_uuid().as_bytes());
    bytes[18..34].copy_from_slice(continuation_epoch.as_uuid().as_bytes());
    bytes[34..].copy_from_slice(&position.to_be_bytes());
    URL_SAFE_NO_PAD.encode(bytes)
}

fn decode_storage_token(
    token: &str,
    expected_kind: TokenKind,
    store_id: &EntityId,
    continuation_epoch: &EntityId,
) -> Option<u64> {
    let bytes = URL_SAFE_NO_PAD.decode(token).ok()?;
    if bytes.len() != STORAGE_TOKEN_LENGTH
        || bytes[0] != STORAGE_TOKEN_VERSION
        || bytes[1] != expected_kind as u8
        || bytes[2..18] != store_id.as_uuid().as_bytes()[..]
        || bytes[18..34] != continuation_epoch.as_uuid().as_bytes()[..]
    {
        return None;
    }
    let position = <[u8; 8]>::try_from(&bytes[34..]).ok()?;
    Some(u64::from_be_bytes(position))
}

/// An immutable Event paired with its backend append cursor.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StoredEvent {
    cursor: EventCursor,
    event: Event,
}

impl StoredEvent {
    /// Creates a stored event from an opaque cursor and immutable event fact.
    pub const fn new(cursor: EventCursor, event: Event) -> Self {
        Self { cursor, event }
    }

    /// Returns the opaque append cursor for this record.
    #[must_use]
    pub const fn cursor(&self) -> &EventCursor {
        &self.cursor
    }

    /// Returns the stored protocol event.
    #[must_use]
    pub const fn event(&self) -> &Event {
        &self.event
    }

    /// Consumes the record and returns its protocol event.
    #[must_use]
    pub fn into_event(self) -> Event {
        self.event
    }
}

/// One append-ordered page of stored events.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EventPage {
    events: Vec<StoredEvent>,
    next_cursor: Option<EventCursor>,
}

impl EventPage {
    /// Creates an append-ordered query page.
    pub const fn new(events: Vec<StoredEvent>, next_cursor: Option<EventCursor>) -> Self {
        Self {
            events,
            next_cursor,
        }
    }

    /// Returns the stored events in append order.
    #[must_use]
    pub fn events(&self) -> &[StoredEvent] {
        &self.events
    }

    /// Returns the cursor for requesting the next page, when more matches exist.
    #[must_use]
    pub const fn next_cursor(&self) -> Option<&EventCursor> {
        self.next_cursor.as_ref()
    }

    /// Returns the number of records in this page.
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Returns whether this page contains no records.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Consumes the page and returns its stored records.
    #[must_use]
    pub fn into_events(self) -> Vec<StoredEvent> {
        self.events
    }
}

#[cfg(test)]
mod tests {
    use orbitrelay_core::EntityId;

    use super::{EventCursor, EventStoreCheckpoint};

    #[test]
    fn cursor_round_trips_without_debugging_its_token() {
        let cursor = EventCursor::for_memory(&EntityId::new(), 7);
        let encoded = serde_json::to_string(&cursor).expect("cursor should serialize");
        let decoded: EventCursor =
            serde_json::from_str(&encoded).expect("cursor should deserialize");

        assert_eq!(decoded, cursor);
        assert_eq!(format!("{cursor:?}"), "EventCursor(..)");
        assert!(!encoded.contains("memory"));
        assert!(!encoded.contains(&EntityId::new().to_string()));
    }

    #[test]
    fn checkpoint_round_trips_as_a_distinct_redacted_token() {
        let checkpoint = EventStoreCheckpoint::for_memory(&EntityId::new(), 0);
        let encoded = serde_json::to_string(&checkpoint).expect("checkpoint should serialize");
        let decoded: EventStoreCheckpoint =
            serde_json::from_str(&encoded).expect("checkpoint should deserialize");

        assert_eq!(decoded, checkpoint);
        assert_eq!(format!("{checkpoint:?}"), "EventStoreCheckpoint(..)");
    }
}
