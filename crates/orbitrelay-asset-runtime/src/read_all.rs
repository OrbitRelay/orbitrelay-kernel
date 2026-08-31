//! Bounded convenience loading built on top of range reads.

use bytes::Bytes;
use orbitrelay_asset::AssetId;

use crate::{AssetByteRange, AssetCatalog, AssetReadAllError, AssetReader};

const READ_CHUNK_SIZE: u64 = 64 * 1024;

/// Reads an entire Asset through bounded range requests.
///
/// This helper is deliberately not the `AssetReader` contract. It first checks
/// the descriptor length against `max_bytes`, then requests fixed-size ranges
/// until EOF. The caller must choose an explicit memory limit.
pub async fn read_asset_fully(
    catalog: &dyn AssetCatalog,
    reader: &dyn AssetReader,
    asset_id: &AssetId,
    max_bytes: u64,
) -> Result<Bytes, AssetReadAllError> {
    let descriptor = catalog
        .get_asset(asset_id)
        .await
        .map_err(|error| AssetReadAllError::CatalogUnavailable {
            detail: error.to_string(),
        })?
        .ok_or_else(|| AssetReadAllError::NotFound {
            asset_id: asset_id.clone(),
        })?;
    let total_length = descriptor.byte_length();
    if total_length > max_bytes {
        return Err(AssetReadAllError::AssetTooLarge {
            asset_id: asset_id.clone(),
            length: total_length,
            max_bytes,
        });
    }
    if usize::try_from(total_length).is_err() {
        return Err(AssetReadAllError::LengthOverflow {
            asset_id: asset_id.clone(),
            length: total_length,
        });
    }

    let mut output = Vec::new();
    let mut offset = 0_u64;
    while offset < total_length {
        let length = (total_length - offset).min(READ_CHUNK_SIZE);
        let range =
            AssetByteRange::new(offset, length).map_err(|_| AssetReadAllError::InvalidChunk {
                asset_id: asset_id.clone(),
                reason: "helper generated an invalid range",
            })?;
        let chunk = reader.read_range(asset_id, range).await?;
        if chunk.offset() != offset || chunk.total_length() != total_length {
            return Err(AssetReadAllError::InvalidChunk {
                asset_id: asset_id.clone(),
                reason: "chunk offset or total length disagrees with metadata",
            });
        }
        if u64::try_from(chunk.bytes().len()).ok() != Some(length) {
            return Err(AssetReadAllError::InvalidChunk {
                asset_id: asset_id.clone(),
                reason: "chunk byte length disagrees with requested range",
            });
        }
        let next_offset = chunk.next_offset().ok_or(AssetReadAllError::InvalidChunk {
            asset_id: asset_id.clone(),
            reason: "chunk next offset overflowed",
        })?;
        if next_offset <= offset || next_offset > total_length {
            return Err(AssetReadAllError::InvalidChunk {
                asset_id: asset_id.clone(),
                reason: "chunk did not make bounded forward progress",
            });
        }
        output.extend_from_slice(chunk.bytes());
        offset = next_offset;
    }

    if u64::try_from(output.len()).ok() != Some(total_length) {
        return Err(AssetReadAllError::InvalidChunk {
            asset_id: asset_id.clone(),
            reason: "assembled byte length disagrees with metadata",
        });
    }
    Ok(Bytes::from(output))
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use orbitrelay_asset::{AssetId, ContentHash, SourceAssetDescriptor};
    use sha2::{Digest, Sha256};

    use super::read_asset_fully;
    use crate::{AssetInsertOutcome, AssetReadAllError, MemoryAssetStore};

    fn descriptor(bytes: &[u8]) -> SourceAssetDescriptor {
        let digest = Sha256::digest(bytes);
        let mut digest_bytes = [0_u8; 32];
        digest_bytes.copy_from_slice(&digest);
        SourceAssetDescriptor::new(
            AssetId::new(),
            "application/octet-stream",
            bytes.len() as u64,
            ContentHash::from_bytes(digest_bytes),
            None,
        )
        .expect("descriptor should be valid")
    }

    #[tokio::test]
    async fn reads_large_asset_through_bounded_ranges() {
        let bytes = Bytes::from(vec![7_u8; 70_000]);
        let descriptor = descriptor(&bytes);
        let asset_id = descriptor.asset_id().clone();
        let store = MemoryAssetStore::new();
        assert_eq!(
            store
                .insert_verified(descriptor, bytes.clone())
                .expect("insert should succeed"),
            AssetInsertOutcome::Inserted
        );

        let result = read_asset_fully(&store, &store, &asset_id, 100_000)
            .await
            .expect("full read should succeed");
        assert_eq!(result, bytes);
    }

    #[tokio::test]
    async fn enforces_explicit_memory_limit_before_reading() {
        let bytes = Bytes::from_static(b"asset");
        let descriptor = descriptor(&bytes);
        let asset_id = descriptor.asset_id().clone();
        let store = MemoryAssetStore::new();
        store
            .insert_verified(descriptor, bytes)
            .expect("insert should succeed");

        assert!(matches!(
            read_asset_fully(&store, &store, &asset_id, 4).await,
            Err(AssetReadAllError::AssetTooLarge { .. })
        ));
    }

    #[tokio::test]
    async fn reads_known_zero_byte_asset_as_empty_bytes() {
        let bytes = Bytes::new();
        let descriptor = descriptor(&bytes);
        let asset_id = descriptor.asset_id().clone();
        let store = MemoryAssetStore::new();
        store
            .insert_verified(descriptor, bytes)
            .expect("empty insert should succeed");

        assert!(read_asset_fully(&store, &store, &asset_id, 0)
            .await
            .expect("empty full read should succeed")
            .is_empty());
    }
}
