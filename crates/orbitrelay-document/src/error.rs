//! Errors produced by pure Document domain values.

use thiserror::Error;

use orbitrelay_canvas::CanvasId;

use crate::PageId;

/// An invariant violation while constructing or decoding a Document value.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum DocumentError {
    /// A title was empty after trimming whitespace.
    #[error("document title must not be empty")]
    InvalidTitle,

    /// A page geometry dimension was not finite and positive.
    #[error("invalid page geometry dimension `{dimension}`")]
    InvalidPageGeometry {
        /// The invalid dimension name.
        dimension: &'static str,
    },

    /// A page rotation value was not one of the four supported quarter turns.
    #[error("invalid page rotation {degrees}; expected 0, 90, 180, or 270")]
    InvalidPageRotation {
        /// The rejected degree value.
        degrees: u16,
    },

    /// A Document descriptor contained no pages.
    #[error("PDF document must contain at least one page")]
    EmptyPages,

    /// A PageId occurred more than once in one Document.
    #[error("duplicate PageId {page_id}")]
    DuplicatePageId {
        /// The repeated page identity.
        page_id: PageId,
    },

    /// A page index occurred more than once in one Document.
    #[error("duplicate page index {page_index}")]
    DuplicatePageIndex {
        /// The repeated page index.
        page_index: u32,
    },

    /// A page index did not match the required zero-based contiguous order.
    #[error("page index {actual} is out of order; expected {expected}")]
    NonContiguousPageIndex {
        /// The required index at this vector position.
        expected: u32,
        /// The supplied index at this vector position.
        actual: u32,
    },

    /// A page CanvasId occurred more than once in one Document.
    #[error("invalid Canvas mapping: duplicate overlay CanvasId {canvas_id}")]
    DuplicateCanvasId {
        /// The repeated overlay Canvas identity.
        canvas_id: CanvasId,
    },

    /// A generic descriptor invariant failed.
    #[error("invalid document descriptor: {reason}")]
    InvalidDescriptor {
        /// A stable, safe description of the failed invariant.
        reason: &'static str,
    },
}
