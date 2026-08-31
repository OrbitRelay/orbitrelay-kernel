//! Read-only Asset access ports and a memory-backed adapter.
//!
//! This crate intentionally contains no upload, replacement, deletion,
//! filesystem, HTTP, or object-storage implementation. The immutable values
//! used by the ports come from [`orbitrelay_asset`].

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod catalog;
mod error;
mod memory;
mod range;
mod read_all;
mod reader;

pub use catalog::AssetCatalog;
pub use error::{
    AssetCatalogError, AssetChunkError, AssetInsertOutcome, AssetRangeError, AssetReadAllError,
    AssetReadError, MemoryAssetStoreError,
};
pub use memory::MemoryAssetStore;
pub use range::AssetByteRange;
pub use read_all::read_asset_fully;
pub use reader::{AssetByteChunk, AssetReader};
