//! PDF metadata and page-geometry inspection through immutable Asset ports.
//!
//! This crate deliberately stops at PDF-derived metadata. It does not create
//! OrbitRelay collaboration identities, read filesystem paths, render pages,
//! or implement upload and storage behavior.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod error;
mod inspector;
mod limits;
mod metadata;
mod parser;

pub use error::PdfError;
pub use inspector::PdfInspector;
pub use limits::PdfInspectionLimits;
pub use metadata::{PdfDocumentMetadata, PdfPageMetadata};
