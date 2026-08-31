//! Range-based immutable byte-read port and result value.

use async_trait::async_trait;
use bytes::Bytes;
use orbitrelay_asset::AssetId;

use crate::{AssetByteRange, AssetChunkError, AssetReadError};

/// Reads immutable Asset bytes for one validated byte range.
#[async_trait]
pub trait AssetReader: Send + Sync {
    /// Reads `[offset, min(offset + length, total_length))`.
    ///
    /// A range beginning at the end of an Asset returns an empty EOF chunk.
    /// A range beyond the end returns [`AssetReadError::RangeOutOfBounds`].
    async fn read_range(
        &self,
        asset_id: &AssetId,
        range: AssetByteRange,
    ) -> Result<AssetByteChunk, AssetReadError>;
}

/// One immutable range-read result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetByteChunk {
    offset: u64,
    bytes: Bytes,
    total_length: u64,
}

impl AssetByteChunk {
    /// Creates a validated chunk for an Asset with `total_length` bytes.
    pub fn new(offset: u64, bytes: Bytes, total_length: u64) -> Result<Self, AssetChunkError> {
        if offset > total_length {
            return Err(AssetChunkError::OffsetBeyondTotal {
                offset,
                total_length,
            });
        }
        let byte_length =
            u64::try_from(bytes.len()).map_err(|_| AssetChunkError::LengthOverflow {
                byte_length: bytes.len(),
            })?;
        let next_offset =
            offset
                .checked_add(byte_length)
                .ok_or(AssetChunkError::OffsetOverflow {
                    offset,
                    byte_length,
                })?;
        if next_offset > total_length {
            return Err(AssetChunkError::BytesBeyondTotal {
                offset,
                byte_length,
                total_length,
            });
        }
        if bytes.is_empty() && offset < total_length {
            return Err(AssetChunkError::EmptyBeforeEof {
                offset,
                total_length,
            });
        }

        Ok(Self {
            offset,
            bytes,
            total_length,
        })
    }

    /// Returns the offset of the first byte in this chunk.
    #[must_use]
    pub const fn offset(&self) -> u64 {
        self.offset
    }

    /// Returns the immutable bytes in this chunk.
    #[must_use]
    pub fn bytes(&self) -> &Bytes {
        &self.bytes
    }

    /// Returns the complete Asset length reported by the reader.
    #[must_use]
    pub const fn total_length(&self) -> u64 {
        self.total_length
    }

    /// Returns the offset immediately after this chunk, or `None` on overflow.
    #[must_use]
    pub fn next_offset(&self) -> Option<u64> {
        let byte_length = u64::try_from(self.bytes.len()).ok()?;
        self.offset.checked_add(byte_length)
    }

    /// Reports whether this chunk reaches the Asset EOF.
    #[must_use]
    pub fn is_eof(&self) -> bool {
        self.next_offset() == Some(self.total_length)
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::AssetByteChunk;
    use crate::AssetChunkError;

    #[test]
    fn reports_next_offset_and_eof() {
        let chunk =
            AssetByteChunk::new(4, Bytes::from_static(b"4567"), 10).expect("chunk should be valid");

        assert_eq!(chunk.offset(), 4);
        assert_eq!(chunk.total_length(), 10);
        assert_eq!(chunk.bytes().as_ref(), b"4567");
        assert_eq!(chunk.next_offset(), Some(8));
        assert!(!chunk.is_eof());

        let eof = AssetByteChunk::new(10, Bytes::new(), 10).expect("EOF should be valid");
        assert_eq!(eof.next_offset(), Some(10));
        assert!(eof.is_eof());
    }

    #[test]
    fn rejects_chunks_outside_reported_total() {
        assert!(matches!(
            AssetByteChunk::new(11, Bytes::new(), 10),
            Err(AssetChunkError::OffsetBeyondTotal { .. })
        ));
        assert!(matches!(
            AssetByteChunk::new(8, Bytes::from_static(b"789"), 10),
            Err(AssetChunkError::BytesBeyondTotal { .. })
        ));
        assert!(matches!(
            AssetByteChunk::new(8, Bytes::new(), 10),
            Err(AssetChunkError::EmptyBeforeEof { .. })
        ));
    }
}
