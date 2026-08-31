//! Errors produced by pure Asset domain values.

use thiserror::Error;

/// An invariant violation while constructing or decoding an Asset value.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum AssetError {
    /// The supplied value is not a valid canonical SHA-256 hexadecimal digest.
    #[error("invalid content hash: {reason}")]
    InvalidContentHash {
        /// A stable, safe description of the validation failure.
        reason: &'static str,
    },

    /// The media type was empty after trimming whitespace.
    #[error("media type must not be empty")]
    InvalidMediaType,

    /// The optional original filename was empty after trimming whitespace.
    #[error("original filename must not be empty")]
    InvalidOriginalFilename,
}
