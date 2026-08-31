//! Pure immutable asset metadata for OrbitRelay.
//!
//! This crate deliberately does not read, write, or locate bytes. Storage
//! adapters may use [`AssetId`] to address an asset and may keep backend-only
//! object keys outside this domain model.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod descriptor;
mod error;
mod hash;
mod id;

pub use descriptor::SourceAssetDescriptor;
pub use error::AssetError;
pub use hash::ContentHash;
pub use id::AssetId;
