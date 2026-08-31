//! In-memory development and test adapter for immutable Assets.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use bytes::Bytes;
use orbitrelay_asset::{AssetId, ContentHash, SourceAssetDescriptor};
use sha2::{Digest, Sha256};

use crate::{
    AssetByteChunk, AssetByteRange, AssetCatalog, AssetCatalogError, AssetInsertOutcome,
    AssetReadError, AssetReader, MemoryAssetStoreError,
};

struct MemoryAssetRecord {
    descriptor: SourceAssetDescriptor,
    bytes: Bytes,
}

/// A cloneable, shared in-memory Asset metadata and byte-read adapter.
///
/// `insert_verified` is intentionally an adapter setup/ingest method, not a
/// replacement for a future upload protocol. There is no replace or delete
/// operation.
#[derive(Clone, Default)]
pub struct MemoryAssetStore {
    records: Arc<RwLock<HashMap<AssetId, MemoryAssetRecord>>>,
}

impl MemoryAssetStore {
    /// Creates an empty memory adapter.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts an Asset only when its length and SHA-256 hash verify.
    ///
    /// Identical repeated inserts return [`AssetInsertOutcome::Existing`]. A
    /// different descriptor or byte sequence for an existing AssetId returns
    /// [`MemoryAssetStoreError::AssetConflict`] and never overwrites data.
    pub fn insert_verified(
        &self,
        descriptor: SourceAssetDescriptor,
        bytes: Bytes,
    ) -> Result<AssetInsertOutcome, MemoryAssetStoreError> {
        let actual_length =
            u64::try_from(bytes.len()).map_err(|_| MemoryAssetStoreError::LengthOverflow {
                actual: bytes.len(),
            })?;
        if descriptor.byte_length() != actual_length {
            return Err(MemoryAssetStoreError::LengthMismatch {
                asset_id: descriptor.asset_id().clone(),
                expected: descriptor.byte_length(),
                actual: actual_length,
            });
        }

        let actual_hash = sha256(&bytes);
        if descriptor.content_hash() != &actual_hash {
            return Err(MemoryAssetStoreError::HashMismatch {
                asset_id: descriptor.asset_id().clone(),
                expected: descriptor.content_hash().clone(),
                actual: actual_hash,
            });
        }

        let asset_id = descriptor.asset_id().clone();
        let mut records = self
            .records
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(existing) = records.get(&asset_id) {
            if existing.descriptor == descriptor && existing.bytes == bytes {
                return Ok(AssetInsertOutcome::Existing);
            }
            return Err(MemoryAssetStoreError::AssetConflict { asset_id });
        }

        records.insert(asset_id, MemoryAssetRecord { descriptor, bytes });
        Ok(AssetInsertOutcome::Inserted)
    }
}

#[async_trait]
impl AssetCatalog for MemoryAssetStore {
    async fn get_asset(
        &self,
        asset_id: &AssetId,
    ) -> Result<Option<SourceAssetDescriptor>, AssetCatalogError> {
        let records = self
            .records
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(records
            .get(asset_id)
            .map(|record| record.descriptor.clone()))
    }
}

#[async_trait]
impl AssetReader for MemoryAssetStore {
    async fn read_range(
        &self,
        asset_id: &AssetId,
        range: AssetByteRange,
    ) -> Result<AssetByteChunk, AssetReadError> {
        let records = self
            .records
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let record = records
            .get(asset_id)
            .ok_or_else(|| AssetReadError::NotFound {
                asset_id: asset_id.clone(),
            })?;
        let total_length = record.descriptor.byte_length();
        if range.offset() > total_length {
            return Err(AssetReadError::RangeOutOfBounds {
                asset_id: asset_id.clone(),
                offset: range.offset(),
                total_length,
            });
        }

        if range.offset() == total_length {
            return AssetByteChunk::new(range.offset(), Bytes::new(), total_length).map_err(|_| {
                AssetReadError::Unavailable {
                    detail: "memory asset produced an invalid EOF chunk".to_owned(),
                }
            });
        }

        let requested_end = range
            .end_offset()
            .ok_or_else(|| AssetReadError::Unavailable {
                detail: "memory asset range overflowed unexpectedly".to_owned(),
            })?;
        let actual_end = requested_end.min(total_length);
        let start = usize::try_from(range.offset()).map_err(|_| AssetReadError::Unavailable {
            detail: "asset offset cannot be represented on this platform".to_owned(),
        })?;
        let end = usize::try_from(actual_end).map_err(|_| AssetReadError::Unavailable {
            detail: "asset range end cannot be represented on this platform".to_owned(),
        })?;
        let bytes = record.bytes.slice(start..end);
        AssetByteChunk::new(range.offset(), bytes, total_length).map_err(|_| {
            AssetReadError::Unavailable {
                detail: "memory asset produced an invalid range chunk".to_owned(),
            }
        })
    }
}

fn sha256(bytes: &Bytes) -> ContentHash {
    let digest = Sha256::digest(bytes.as_ref());
    let mut digest_bytes = [0_u8; 32];
    digest_bytes.copy_from_slice(&digest);
    ContentHash::from_bytes(digest_bytes)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bytes::Bytes;
    use orbitrelay_asset::{AssetId, ContentHash, SourceAssetDescriptor};
    use sha2::{Digest, Sha256};

    use super::MemoryAssetStore;
    use crate::{
        AssetByteRange, AssetCatalog, AssetInsertOutcome, AssetReadError, AssetReader,
        MemoryAssetStoreError,
    };

    fn descriptor(asset_id: AssetId, bytes: &[u8]) -> SourceAssetDescriptor {
        let digest = Sha256::digest(bytes);
        let mut digest_bytes = [0_u8; 32];
        digest_bytes.copy_from_slice(&digest);
        SourceAssetDescriptor::new(
            asset_id,
            "application/octet-stream",
            bytes.len() as u64,
            ContentHash::from_bytes(digest_bytes),
            Some("payload.bin".to_owned()),
        )
        .expect("descriptor should be valid")
    }

    #[tokio::test]
    async fn reads_ranges_with_short_final_chunk_and_eof() {
        let bytes = Bytes::from_static(b"0123456789");
        let asset_id = AssetId::new();
        let store = MemoryAssetStore::new();
        store
            .insert_verified(descriptor(asset_id.clone(), &bytes), bytes)
            .expect("insert should succeed");

        let first = store
            .read_range(&asset_id, AssetByteRange::new(0, 4).unwrap())
            .await
            .expect("first range should succeed");
        assert_eq!(first.bytes().as_ref(), b"0123");
        assert_eq!(first.offset(), 0);
        assert_eq!(first.total_length(), 10);
        assert!(!first.is_eof());

        let middle = store
            .read_range(&asset_id, AssetByteRange::new(4, 4).unwrap())
            .await
            .expect("middle range should succeed");
        assert_eq!(middle.bytes().as_ref(), b"4567");

        let final_chunk = store
            .read_range(&asset_id, AssetByteRange::new(8, 4).unwrap())
            .await
            .expect("final range should be short");
        assert_eq!(final_chunk.bytes().as_ref(), b"89");
        assert!(final_chunk.is_eof());

        let eof = store
            .read_range(&asset_id, AssetByteRange::new(10, 4).unwrap())
            .await
            .expect("range at EOF should succeed");
        assert!(eof.bytes().is_empty());
        assert_eq!(eof.offset(), 10);
        assert!(eof.is_eof());

        assert!(matches!(
            store
                .read_range(&asset_id, AssetByteRange::new(11, 4).unwrap())
                .await,
            Err(AssetReadError::RangeOutOfBounds { .. })
        ));
    }

    #[tokio::test]
    async fn unknown_asset_is_not_the_same_as_empty_asset() {
        let store = MemoryAssetStore::new();
        let unknown = AssetId::new();
        assert!(matches!(
            store
                .read_range(&unknown, AssetByteRange::new(0, 1).unwrap())
                .await,
            Err(AssetReadError::NotFound { .. })
        ));

        let empty = Bytes::new();
        let empty_id = AssetId::new();
        store
            .insert_verified(descriptor(empty_id.clone(), &empty), empty)
            .expect("empty Asset should be valid");
        let eof = store
            .read_range(&empty_id, AssetByteRange::new(0, 1).unwrap())
            .await
            .expect("empty Asset should return EOF");
        assert!(eof.bytes().is_empty());
        assert!(eof.is_eof());
    }

    #[tokio::test]
    async fn catalog_returns_metadata_and_none_for_unknown_ids() {
        let bytes = Bytes::from_static(b"catalog");
        let asset_id = AssetId::new();
        let descriptor = descriptor(asset_id.clone(), &bytes);
        let store = MemoryAssetStore::new();
        store
            .insert_verified(descriptor.clone(), bytes)
            .expect("insert should succeed");

        assert_eq!(store.get_asset(&asset_id).await.unwrap(), Some(descriptor));
        assert_eq!(store.get_asset(&AssetId::new()).await.unwrap(), None);
    }

    #[tokio::test]
    async fn clone_shares_the_same_memory_state() {
        let bytes = Bytes::from_static(b"shared");
        let asset_id = AssetId::new();
        let descriptor = descriptor(asset_id.clone(), &bytes);
        let first = MemoryAssetStore::new();
        let second = first.clone();
        first
            .insert_verified(descriptor.clone(), bytes)
            .expect("insert should succeed");

        assert_eq!(second.get_asset(&asset_id).await.unwrap(), Some(descriptor));
    }

    #[test]
    fn verifies_length_and_hash_before_locking_and_inserting() {
        let bytes = Bytes::from_static(b"verified");
        let asset_id = AssetId::new();
        let mut wrong_length = descriptor(asset_id.clone(), &bytes);
        wrong_length = SourceAssetDescriptor::new(
            asset_id.clone(),
            "application/octet-stream",
            999,
            wrong_length.content_hash().clone(),
            wrong_length.original_filename().map(str::to_owned),
        )
        .expect("metadata itself is valid");
        let store = MemoryAssetStore::new();
        assert!(matches!(
            store.insert_verified(wrong_length, bytes.clone()),
            Err(MemoryAssetStoreError::LengthMismatch { .. })
        ));

        let wrong_hash = SourceAssetDescriptor::new(
            asset_id,
            "application/octet-stream",
            bytes.len() as u64,
            ContentHash::from_bytes([0; 32]),
            None,
        )
        .expect("metadata itself is valid");
        assert!(matches!(
            store.insert_verified(wrong_hash, bytes),
            Err(MemoryAssetStoreError::HashMismatch { .. })
        ));
    }

    #[tokio::test]
    async fn identical_insert_is_idempotent_and_conflict_never_overwrites() {
        let first_bytes = Bytes::from_static(b"first");
        let asset_id = AssetId::new();
        let first_descriptor = descriptor(asset_id.clone(), &first_bytes);
        let store = MemoryAssetStore::new();

        assert_eq!(
            store
                .insert_verified(first_descriptor.clone(), first_bytes.clone())
                .expect("first insert should succeed"),
            AssetInsertOutcome::Inserted
        );
        assert_eq!(
            store
                .insert_verified(first_descriptor, first_bytes)
                .expect("same insert should be idempotent"),
            AssetInsertOutcome::Existing
        );

        let conflicting_bytes = Bytes::from_static(b"other");
        assert!(matches!(
            store.insert_verified(
                descriptor(asset_id.clone(), &conflicting_bytes),
                conflicting_bytes
            ),
            Err(MemoryAssetStoreError::AssetConflict { .. })
        ));
        let retained = store
            .read_range(&asset_id, AssetByteRange::new(0, 5).unwrap())
            .await
            .expect("retained bytes should be readable");
        assert_eq!(retained.bytes().as_ref(), b"first");

        let descriptor_conflict = SourceAssetDescriptor::new(
            asset_id.clone(),
            "text/plain",
            5,
            descriptor(asset_id.clone(), b"first")
                .content_hash()
                .clone(),
            None,
        )
        .expect("conflicting metadata is structurally valid");
        assert!(matches!(
            store.insert_verified(descriptor_conflict, Bytes::from_static(b"first")),
            Err(MemoryAssetStoreError::AssetConflict { .. })
        ));
        let retained = store
            .read_range(&asset_id, AssetByteRange::new(0, 5).unwrap())
            .await
            .expect("retained bytes should remain readable");
        assert_eq!(retained.bytes().as_ref(), b"first");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn concurrent_reads_are_safe() {
        let bytes = Bytes::from_static(b"0123456789");
        let asset_id = AssetId::new();
        let store = Arc::new(MemoryAssetStore::new());
        store
            .insert_verified(descriptor(asset_id.clone(), &bytes), bytes)
            .expect("insert should succeed");

        let first_store = store.clone();
        let first_id = asset_id.clone();
        let first = tokio::spawn(async move {
            first_store
                .read_range(&first_id, AssetByteRange::new(0, 5).unwrap())
                .await
        });
        let second_store = store.clone();
        let second = tokio::spawn(async move {
            second_store
                .read_range(&asset_id, AssetByteRange::new(5, 5).unwrap())
                .await
        });
        let (first, second) = tokio::join!(first, second);
        assert_eq!(first.unwrap().unwrap().bytes().as_ref(), b"01234");
        assert_eq!(second.unwrap().unwrap().bytes().as_ref(), b"56789");
    }

    #[tokio::test]
    async fn concurrent_identical_inserts_have_one_record() {
        let bytes = Bytes::from_static(b"same");
        let asset_id = AssetId::new();
        let descriptor = descriptor(asset_id, &bytes);
        let store = Arc::new(MemoryAssetStore::new());
        let first_store = store.clone();
        let first_descriptor = descriptor.clone();
        let first_bytes = bytes.clone();
        let second_store = store.clone();
        let second_descriptor = descriptor;
        let second_bytes = bytes;

        let first = tokio::task::spawn_blocking(move || {
            first_store.insert_verified(first_descriptor, first_bytes)
        });
        let second = tokio::task::spawn_blocking(move || {
            second_store.insert_verified(second_descriptor, second_bytes)
        });
        let (first, second) = tokio::join!(first, second);
        let outcomes = [first.unwrap().unwrap(), second.unwrap().unwrap()];
        assert!(outcomes.contains(&AssetInsertOutcome::Inserted));
        assert!(outcomes.contains(&AssetInsertOutcome::Existing));
    }

    #[tokio::test]
    async fn concurrent_conflicting_inserts_never_partially_overwrite() {
        let first_bytes = Bytes::from_static(b"first");
        let second_bytes = Bytes::from_static(b"other");
        let asset_id = AssetId::new();
        let first_descriptor = descriptor(asset_id.clone(), &first_bytes);
        let second_descriptor = descriptor(asset_id.clone(), &second_bytes);
        let store = Arc::new(MemoryAssetStore::new());
        let first_store = store.clone();
        let second_store = store.clone();

        let first = tokio::task::spawn_blocking(move || {
            first_store.insert_verified(first_descriptor, first_bytes)
        });
        let second = tokio::task::spawn_blocking(move || {
            second_store.insert_verified(second_descriptor, second_bytes)
        });
        let (first, second) = tokio::join!(first, second);
        let outcomes = [first.unwrap(), second.unwrap()];
        assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| matches!(
                    outcome,
                    Err(MemoryAssetStoreError::AssetConflict { .. })
                ))
                .count(),
            1
        );
        let retained = store
            .read_range(&asset_id, AssetByteRange::new(0, 5).unwrap())
            .await
            .expect("one complete value should remain readable");
        assert!(retained.bytes().as_ref() == b"first" || retained.bytes().as_ref() == b"other");
    }
}
