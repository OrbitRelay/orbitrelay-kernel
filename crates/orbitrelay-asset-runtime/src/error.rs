//! Errors and outcomes for Asset access adapters.

use orbitrelay_asset::{AssetId, ContentHash};
use thiserror::Error;

/// A failure while looking up immutable Asset metadata.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum AssetCatalogError {
    /// The metadata backend could not serve the lookup.
    #[error("asset catalog unavailable: {detail}")]
    Unavailable {
        /// A safe diagnostic without backend paths or secrets.
        detail: String,
    },
}

/// A failure while reading immutable Asset bytes.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum AssetReadError {
    /// No Asset exists for the requested identity.
    #[error("asset {asset_id} was not found")]
    NotFound {
        /// The requested Asset identity.
        asset_id: AssetId,
    },

    /// The range begins beyond the end of the Asset.
    #[error("asset {asset_id} range offset {offset} is beyond total length {total_length}")]
    RangeOutOfBounds {
        /// The requested Asset identity.
        asset_id: AssetId,
        /// The invalid range offset.
        offset: u64,
        /// The immutable Asset length.
        total_length: u64,
    },

    /// The byte backend could not serve the read.
    #[error("asset read unavailable: {detail}")]
    Unavailable {
        /// A safe diagnostic without backend paths or secrets.
        detail: String,
    },
}

/// A malformed range supplied to an Asset reader.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum AssetRangeError {
    /// A range must request at least one byte.
    #[error("asset byte range length must be greater than zero")]
    ZeroLength,

    /// The exclusive end offset overflowed `u64`.
    #[error("asset byte range offset {offset} plus length {length} overflows u64")]
    OffsetOverflow {
        /// The range offset.
        offset: u64,
        /// The requested length.
        length: u64,
    },
}

/// A malformed chunk returned by an Asset reader implementation.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum AssetChunkError {
    /// The chunk offset is after the reported Asset end.
    #[error("chunk offset {offset} is beyond total length {total_length}")]
    OffsetBeyondTotal {
        /// The chunk offset.
        offset: u64,
        /// The reported Asset length.
        total_length: u64,
    },

    /// The platform byte length could not be represented as `u64`.
    #[error("chunk byte length {byte_length} cannot be represented as u64")]
    LengthOverflow {
        /// The platform byte length.
        byte_length: usize,
    },

    /// Adding the chunk length to its offset overflowed `u64`.
    #[error("chunk offset {offset} plus byte length {byte_length} overflows u64")]
    OffsetOverflow {
        /// The chunk offset.
        offset: u64,
        /// The chunk byte length.
        byte_length: u64,
    },

    /// The chunk extends beyond the reported Asset length.
    #[error(
        "chunk at offset {offset} with length {byte_length} exceeds total length {total_length}"
    )]
    BytesBeyondTotal {
        /// The chunk offset.
        offset: u64,
        /// The chunk byte length.
        byte_length: u64,
        /// The reported Asset length.
        total_length: u64,
    },

    /// An empty chunk was returned before the Asset EOF.
    #[error("empty chunk at offset {offset} precedes total length {total_length}")]
    EmptyBeforeEof {
        /// The chunk offset.
        offset: u64,
        /// The reported Asset length.
        total_length: u64,
    },
}

/// Result of inserting verified bytes into a MemoryAssetStore.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetInsertOutcome {
    /// The Asset identity was newly inserted.
    Inserted,
    /// An identical descriptor and byte sequence already existed.
    Existing,
}

/// A failure while verifying or inserting data into the memory adapter.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum MemoryAssetStoreError {
    /// The supplied descriptor length differs from the actual bytes.
    #[error("asset {asset_id} length mismatch: expected {expected}, actual {actual}")]
    LengthMismatch {
        /// The Asset identity being inserted.
        asset_id: AssetId,
        /// The descriptor byte length.
        expected: u64,
        /// The actual byte length.
        actual: u64,
    },

    /// The supplied descriptor hash differs from SHA-256(bytes).
    #[error("asset {asset_id} content hash mismatch")]
    HashMismatch {
        /// The Asset identity being inserted.
        asset_id: AssetId,
        /// The descriptor hash.
        expected: ContentHash,
        /// The calculated hash.
        actual: ContentHash,
    },

    /// The platform byte length could not be represented as `u64`.
    #[error("asset byte length {actual} cannot be represented as u64")]
    LengthOverflow {
        /// The platform byte length.
        actual: usize,
    },

    /// The identity already exists with different metadata or bytes.
    #[error("asset {asset_id} conflicts with immutable existing content")]
    AssetConflict {
        /// The conflicting Asset identity.
        asset_id: AssetId,
    },
}

/// Failure while reading one Asset completely through bounded ranges.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum AssetReadAllError {
    /// The metadata lookup backend failed.
    #[error("asset catalog lookup failed: {detail}")]
    CatalogUnavailable {
        /// A safe diagnostic without backend details.
        detail: String,
    },

    /// The requested Asset does not exist.
    #[error("asset {asset_id} was not found")]
    NotFound {
        /// The missing Asset identity.
        asset_id: AssetId,
    },

    /// The Asset exceeds the caller's explicit in-memory limit.
    #[error("asset {asset_id} length {length} exceeds read limit {max_bytes}")]
    AssetTooLarge {
        /// The Asset identity.
        asset_id: AssetId,
        /// The metadata byte length.
        length: u64,
        /// The caller-provided maximum.
        max_bytes: u64,
    },

    /// The Asset length cannot be represented by this process's Vec.
    #[error("asset {asset_id} length {length} cannot be loaded into memory")]
    LengthOverflow {
        /// The Asset identity.
        asset_id: AssetId,
        /// The metadata byte length.
        length: u64,
    },

    /// The reader returned an access error.
    #[error(transparent)]
    Reader(#[from] AssetReadError),

    /// The reader returned a non-progressing or inconsistent chunk.
    #[error("asset {asset_id} returned an invalid read chunk: {reason}")]
    InvalidChunk {
        /// The affected Asset identity.
        asset_id: AssetId,
        /// A stable description of the inconsistency.
        reason: &'static str,
    },
}
