//! Pure Document and PDF-page structure for OrbitRelay.
//!
//! The crate accepts normalized page display geometry and maps each page to a
//! Canvas overlay. PDF parser types, file bytes, storage, and lifecycle
//! commands remain outside this domain crate.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod descriptor;
mod error;
mod geometry;
mod id;
mod kind;

pub use descriptor::{DocumentDescriptor, DocumentPageDescriptor};
pub use error::DocumentError;
pub use geometry::{PageDisplayGeometry, PageRotation};
pub use id::{DocumentId, PageId};
pub use kind::DocumentType;
