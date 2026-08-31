//! Stable errors at the PDF adapter boundary.

use orbitrelay_asset::AssetId;
use thiserror::Error;

/// A failure while inspecting PDF metadata and page geometry.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum PdfError {
    /// The requested Asset is not registered in the metadata catalog.
    #[error("PDF source asset {asset_id} was not found")]
    AssetNotFound {
        /// Missing source identity.
        asset_id: AssetId,
    },

    /// The metadata catalog could not be queried.
    #[error("PDF source asset metadata is unavailable")]
    AssetUnavailable,

    /// The source Asset exceeds the inspection policy before byte reads begin.
    #[error("PDF source asset is too large: {length} bytes exceeds {max_bytes}")]
    AssetTooLarge {
        /// Descriptor length.
        length: u64,
        /// Configured inspection limit.
        max_bytes: u64,
    },

    /// The immutable Asset reader returned an access or consistency failure.
    #[error("PDF source asset bytes could not be read")]
    ReadFailed,

    /// The bytes are not a parseable PDF document.
    #[error("invalid PDF document")]
    InvalidPdf,

    /// Password-protected PDFs are intentionally unsupported in v0.1.
    #[error("encrypted PDF documents are unsupported")]
    EncryptedUnsupported,

    /// The PDF page tree is missing or structurally malformed.
    #[error("invalid PDF page tree")]
    InvalidPageTree,

    /// The required effective MediaBox is absent.
    #[error("PDF page {page_index} has no MediaBox")]
    MissingMediaBox {
        /// Zero-based page index in logical page-tree order.
        page_index: u32,
    },

    /// A page box could not be normalized to a positive finite rectangle.
    #[error("PDF page {page_index} has invalid page geometry")]
    InvalidPageGeometry {
        /// Zero-based page index in logical page-tree order.
        page_index: u32,
    },

    /// A page rotation is not a multiple of 90 degrees.
    #[error("PDF page {page_index} has invalid rotation {degrees}")]
    InvalidRotation {
        /// Zero-based page index in logical page-tree order.
        page_index: u32,
        /// Raw PDF rotation value.
        degrees: i64,
    },

    /// The page index cannot be represented by the domain's `u32` index.
    #[error("PDF page index exceeds u32")]
    PageIndexOverflow,

    /// The document has more pages than the inspection policy permits.
    #[error("PDF page count exceeds inspection limit {max_pages}")]
    PageLimitExceeded {
        /// Configured page limit.
        max_pages: u32,
    },

    /// A parser decompression stream exceeded its configured bound.
    #[error("PDF parser decompression exceeded {max_bytes} bytes")]
    ParserResourceLimitExceeded {
        /// Configured per-stream bound.
        max_bytes: u64,
    },
}
