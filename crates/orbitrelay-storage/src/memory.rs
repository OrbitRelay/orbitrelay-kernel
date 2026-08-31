//! Thread-safe in-memory EventStore implementation.

use std::{
    collections::HashMap,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

use async_trait::async_trait;
use orbitrelay_core::EntityId;
use orbitrelay_protocol::{Event, EventId};

use crate::{
    EventCursor, EventPage, EventQuery, EventStore, EventStoreCheckpoint, StorageError, StoredEvent,
};

struct State {
    store_id: EntityId,
    records: Vec<StoredEvent>,
    event_indices: HashMap<EventId, usize>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            store_id: EntityId::new(),
            records: Vec::new(),
            event_indices: HashMap::new(),
        }
    }
}

/// A cloneable, thread-safe, append-only in-memory event store.
#[derive(Clone, Default)]
pub struct MemoryEventStore {
    state: Arc<RwLock<State>>,
}

impl MemoryEventStore {
    /// Creates an empty in-memory event store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn read_state(&self) -> RwLockReadGuard<'_, State> {
        self.state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn write_state(&self) -> RwLockWriteGuard<'_, State> {
        self.state
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[async_trait]
impl EventStore for MemoryEventStore {
    async fn capture_checkpoint(&self) -> Result<EventStoreCheckpoint, StorageError> {
        let state = self.read_state();
        Ok(EventStoreCheckpoint::for_memory(
            &state.store_id,
            state.records.len(),
        ))
    }

    async fn append(&self, event: Event) -> Result<StoredEvent, StorageError> {
        let mut state = self.write_state();

        if let Some(index) = state.event_indices.get(event.id()).copied() {
            let existing = &state.records[index];
            if existing.event() == &event {
                return Ok(existing.clone());
            }

            return Err(StorageError::EventConflict {
                event_id: event.id().clone(),
            });
        }

        let cursor = EventCursor::for_memory(&state.store_id, state.records.len() + 1);
        let record = StoredEvent::new(cursor, event);
        let index = state.records.len();
        state
            .event_indices
            .insert(record.event().id().clone(), index);
        state.records.push(record.clone());

        Ok(record)
    }

    async fn get(&self, event_id: &EventId) -> Result<Option<StoredEvent>, StorageError> {
        let state = self.read_state();
        Ok(state
            .event_indices
            .get(event_id)
            .map(|index| state.records[*index].clone()))
    }

    async fn query(&self, query: EventQuery) -> Result<EventPage, StorageError> {
        query.validate()?;
        let state = self.read_state();
        let upper_bound = match query.upper_bound() {
            Some(checkpoint) => checkpoint.memory_position(&state.store_id, state.records.len())?,
            None => state.records.len(),
        };
        let start = match query.after_cursor() {
            Some(cursor) => cursor.memory_position(&state.store_id, state.records.len())?,
            None => 0,
        };
        if start > upper_bound {
            return Err(StorageError::InvalidCursor {
                reason: "cursor is beyond the query checkpoint".to_owned(),
            });
        }

        let mut records = state
            .records
            .get(..upper_bound)
            .expect("validated checkpoint must be within the record range")
            .iter()
            .skip(start)
            .filter(|record| query.matches(record.event()))
            .take(query.limit() + 1)
            .cloned()
            .collect::<Vec<_>>();

        let next_cursor = if records.len() > query.limit() {
            records.truncate(query.limit());
            records.last().map(|record| record.cursor().clone())
        } else {
            None
        };

        Ok(EventPage::new(records, next_cursor))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use orbitrelay_core::{Metadata, Timestamp};
    use orbitrelay_protocol::{ActionId, ActorId, Event, EventId, EventType, Payload, SessionId};

    use super::MemoryEventStore;
    use crate::{EventCursor, EventQuery, EventStore, EventStoreCheckpoint, StorageError};

    fn event(
        event_id: EventId,
        session_id: SessionId,
        actor_id: ActorId,
        event_type: &str,
        occurred_at: i64,
    ) -> Event {
        Event::new(
            event_id,
            session_id,
            actor_id,
            ActionId::new(),
            EventType::new(event_type),
            Timestamp::from_unix_timestamp(occurred_at).expect("timestamp is valid"),
            Payload::new(),
            Metadata::new(),
        )
    }

    #[tokio::test]
    async fn appends_and_gets_event_by_id() {
        let store = MemoryEventStore::new();
        let event = event(
            EventId::new(),
            SessionId::new(),
            ActorId::new(),
            "document.written",
            100,
        );

        let stored = store
            .append(event.clone())
            .await
            .expect("append should succeed");
        let fetched = store
            .get(event.id())
            .await
            .expect("get should succeed")
            .expect("event should exist");

        assert_eq!(stored, fetched);
        assert_eq!(fetched.event(), &event);
    }

    #[tokio::test]
    async fn repeated_identical_event_is_idempotent() {
        let store = MemoryEventStore::new();
        let event = event(
            EventId::new(),
            SessionId::new(),
            ActorId::new(),
            "document.written",
            100,
        );

        let first = store
            .append(event.clone())
            .await
            .expect("first append should succeed");
        let second = store
            .append(event)
            .await
            .expect("identical append should succeed");
        let page = store
            .query(EventQuery::all())
            .await
            .expect("query should succeed");

        assert_eq!(first, second);
        assert_eq!(page.len(), 1);
    }

    #[tokio::test]
    async fn rejects_conflicting_event_content() {
        let store = MemoryEventStore::new();
        let event_id = EventId::new();
        let session_id = SessionId::new();
        let actor_id = ActorId::new();
        store
            .append(event(
                event_id.clone(),
                session_id.clone(),
                actor_id.clone(),
                "document.written",
                100,
            ))
            .await
            .expect("first append should succeed");

        let error = store
            .append(event(
                event_id.clone(),
                session_id,
                actor_id,
                "document.deleted",
                100,
            ))
            .await
            .expect_err("different content must conflict");

        assert_eq!(error, StorageError::EventConflict { event_id });
    }

    #[tokio::test]
    async fn filters_by_session_and_event_type() {
        let store = MemoryEventStore::new();
        let selected_session = SessionId::new();
        let other_session = SessionId::new();
        let actor_id = ActorId::new();
        store
            .append(event(
                EventId::new(),
                selected_session.clone(),
                actor_id.clone(),
                "document.written",
                100,
            ))
            .await
            .expect("append should succeed");
        store
            .append(event(
                EventId::new(),
                selected_session.clone(),
                actor_id.clone(),
                "document.opened",
                101,
            ))
            .await
            .expect("append should succeed");
        store
            .append(event(
                EventId::new(),
                other_session,
                actor_id,
                "document.written",
                102,
            ))
            .await
            .expect("append should succeed");

        let page = store
            .query(
                EventQuery::for_session(selected_session)
                    .with_event_type(EventType::new("document.written")),
            )
            .await
            .expect("query should succeed");

        assert_eq!(page.len(), 1);
        assert_eq!(
            page.events()[0].event().event_type().as_str(),
            "document.written"
        );
    }

    #[tokio::test]
    async fn paginates_by_append_cursor_and_limit() {
        let store = MemoryEventStore::new();
        let session_id = SessionId::new();
        for occurred_at in 100..103 {
            store
                .append(event(
                    EventId::new(),
                    session_id.clone(),
                    ActorId::new(),
                    "event.recorded",
                    occurred_at,
                ))
                .await
                .expect("append should succeed");
        }

        let first = store
            .query(EventQuery::all().with_limit(2))
            .await
            .expect("first page should succeed");
        let cursor = first
            .next_cursor()
            .cloned()
            .expect("more records should be available");
        let second = store
            .query(EventQuery::all().with_limit(2).after(cursor))
            .await
            .expect("second page should succeed");

        assert_eq!(first.len(), 2);
        assert_eq!(second.len(), 1);
        assert!(second.next_cursor().is_none());
        assert_eq!(
            second.events()[0].event().occurred_at().unix_timestamp(),
            102
        );
    }

    #[tokio::test]
    async fn filters_half_open_time_range() {
        let store = MemoryEventStore::new();
        for occurred_at in [100, 200, 300] {
            store
                .append(event(
                    EventId::new(),
                    SessionId::new(),
                    ActorId::new(),
                    "event.recorded",
                    occurred_at,
                ))
                .await
                .expect("append should succeed");
        }

        let page = store
            .query(EventQuery::all().with_time_range(
                Timestamp::from_unix_timestamp(150).expect("timestamp is valid"),
                Timestamp::from_unix_timestamp(300).expect("timestamp is valid"),
            ))
            .await
            .expect("query should succeed");

        assert_eq!(page.len(), 1);
        assert_eq!(page.events()[0].event().occurred_at().unix_timestamp(), 200);
    }

    #[tokio::test]
    async fn rejects_cursor_from_another_store() {
        let first_store = MemoryEventStore::new();
        let second_store = MemoryEventStore::new();
        let record = first_store
            .append(event(
                EventId::new(),
                SessionId::new(),
                ActorId::new(),
                "event.recorded",
                100,
            ))
            .await
            .expect("append should succeed");
        second_store
            .append(event(
                EventId::new(),
                SessionId::new(),
                ActorId::new(),
                "event.recorded",
                100,
            ))
            .await
            .expect("append should succeed");

        let error = second_store
            .query(EventQuery::all().after(record.cursor().clone()))
            .await
            .expect_err("foreign cursor must be rejected");

        assert!(matches!(error, StorageError::InvalidCursor { .. }));
    }

    #[tokio::test]
    async fn rejects_malformed_cursor() {
        let store = MemoryEventStore::new();
        let error = store
            .query(EventQuery::all().after(EventCursor::from_test_token("not-a-cursor")))
            .await
            .expect_err("malformed cursor must be rejected");

        assert!(matches!(error, StorageError::InvalidCursor { .. }));
    }

    #[tokio::test]
    async fn empty_checkpoint_is_a_valid_exclusive_boundary() {
        let store = MemoryEventStore::new();
        let checkpoint = store
            .capture_checkpoint()
            .await
            .expect("empty checkpoint should succeed");
        store
            .append(event(
                EventId::new(),
                SessionId::new(),
                ActorId::new(),
                "event.recorded",
                100,
            ))
            .await
            .expect("append should succeed");

        let page = store
            .query(EventQuery::all().before(checkpoint))
            .await
            .expect("bounded query should succeed");

        assert!(page.is_empty());
        assert!(page.next_cursor().is_none());
    }

    #[tokio::test]
    async fn checkpoint_excludes_later_appends_across_multiple_pages() {
        let store = MemoryEventStore::new();
        let session_id = SessionId::new();
        let mut expected = Vec::new();
        for occurred_at in 100..105 {
            let event = event(
                EventId::new(),
                session_id.clone(),
                ActorId::new(),
                "event.recorded",
                occurred_at,
            );
            expected.push(event.id().clone());
            store.append(event).await.expect("append should succeed");
        }
        let checkpoint = store
            .capture_checkpoint()
            .await
            .expect("checkpoint should succeed");
        let excluded = event(
            EventId::new(),
            session_id,
            ActorId::new(),
            "event.recorded",
            105,
        );
        let excluded_id = excluded.id().clone();
        store
            .append(excluded)
            .await
            .expect("later append should succeed");

        let mut actual = Vec::new();
        let mut cursor = None;
        loop {
            let mut query = EventQuery::all().before(checkpoint.clone()).with_limit(2);
            if let Some(value) = cursor.take() {
                query = query.after(value);
            }
            let page = store.query(query).await.expect("page should succeed");
            actual.extend(
                page.events()
                    .iter()
                    .map(|record| record.event().id().clone()),
            );
            cursor = page.next_cursor().cloned();
            if cursor.is_none() {
                break;
            }
        }

        assert_eq!(actual, expected);
        assert!(!actual.contains(&excluded_id));
    }

    #[tokio::test]
    async fn rejects_checkpoint_from_another_store() {
        let first = MemoryEventStore::new();
        let second = MemoryEventStore::new();
        let checkpoint = first
            .capture_checkpoint()
            .await
            .expect("checkpoint should succeed");

        let error = second
            .query(EventQuery::all().before(checkpoint))
            .await
            .expect_err("foreign checkpoint must fail");

        assert!(matches!(error, StorageError::InvalidCheckpoint { .. }));
    }

    #[tokio::test]
    async fn rejects_cursor_beyond_checkpoint() {
        let store = MemoryEventStore::new();
        store
            .append(event(
                EventId::new(),
                SessionId::new(),
                ActorId::new(),
                "event.recorded",
                100,
            ))
            .await
            .expect("first append should succeed");
        let checkpoint = store
            .capture_checkpoint()
            .await
            .expect("checkpoint should succeed");
        let later = store
            .append(event(
                EventId::new(),
                SessionId::new(),
                ActorId::new(),
                "event.recorded",
                101,
            ))
            .await
            .expect("second append should succeed");

        let error = store
            .query(
                EventQuery::all()
                    .after(later.cursor().clone())
                    .before(checkpoint),
            )
            .await
            .expect_err("cursor beyond checkpoint must fail");

        assert!(matches!(error, StorageError::InvalidCursor { .. }));
    }

    #[tokio::test]
    async fn cursor_and_checkpoint_tokens_are_not_interchangeable() {
        let store = MemoryEventStore::new();
        let record = store
            .append(event(
                EventId::new(),
                SessionId::new(),
                ActorId::new(),
                "event.recorded",
                100,
            ))
            .await
            .expect("append should succeed");
        let cursor_json = serde_json::to_string(record.cursor()).expect("cursor should encode");
        let forged_checkpoint: EventStoreCheckpoint =
            serde_json::from_str(&cursor_json).expect("transparent token should decode");

        let error = store
            .query(EventQuery::all().before(forged_checkpoint))
            .await
            .expect_err("cursor token must not validate as checkpoint");

        assert!(matches!(error, StorageError::InvalidCheckpoint { .. }));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn append_and_checkpoint_have_deterministic_linearization() {
        let append_first = Arc::new(MemoryEventStore::new());
        let (append_done_tx, append_done_rx) = tokio::sync::oneshot::channel();
        let append_store = append_first.clone();
        let append_task = tokio::spawn(async move {
            append_store
                .append(event(
                    EventId::new(),
                    SessionId::new(),
                    ActorId::new(),
                    "event.recorded",
                    100,
                ))
                .await
                .expect("append should succeed");
            append_done_tx.send(()).expect("checkpoint task is alive");
        });
        append_done_rx.await.expect("append task should signal");
        let after_append = append_first
            .capture_checkpoint()
            .await
            .expect("checkpoint should succeed");
        append_task.await.expect("append task should join");
        assert_eq!(
            append_first
                .query(EventQuery::all().before(after_append))
                .await
                .expect("query should succeed")
                .len(),
            1
        );

        let checkpoint_first = Arc::new(MemoryEventStore::new());
        let before_append = checkpoint_first
            .capture_checkpoint()
            .await
            .expect("checkpoint should succeed");
        let (start_append_tx, start_append_rx) = tokio::sync::oneshot::channel();
        let checkpoint_store = checkpoint_first.clone();
        let later_task = tokio::spawn(async move {
            start_append_rx.await.expect("test should release append");
            checkpoint_store
                .append(event(
                    EventId::new(),
                    SessionId::new(),
                    ActorId::new(),
                    "event.recorded",
                    100,
                ))
                .await
                .expect("append should succeed");
        });
        start_append_tx.send(()).expect("append task is alive");
        later_task.await.expect("append task should join");
        assert!(checkpoint_first
            .query(EventQuery::all().before(before_append))
            .await
            .expect("query should succeed")
            .is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn appends_safely_from_multiple_tasks() {
        let store = Arc::new(MemoryEventStore::new());
        let mut tasks = Vec::new();

        for occurred_at in 0..32 {
            let store = store.clone();
            tasks.push(tokio::spawn(async move {
                store
                    .append(event(
                        EventId::new(),
                        SessionId::new(),
                        ActorId::new(),
                        "concurrent.event",
                        occurred_at,
                    ))
                    .await
            }));
        }

        for task in tasks {
            task.await
                .expect("append task should complete")
                .expect("append should succeed");
        }

        let page = store
            .query(EventQuery::all())
            .await
            .expect("query should succeed");
        assert_eq!(page.len(), 32);
    }
}
