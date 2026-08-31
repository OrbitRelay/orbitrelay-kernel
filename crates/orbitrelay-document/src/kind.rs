//! Supported Document types.

use serde::{Deserialize, Serialize};

/// The semantic kind of a Document.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocumentType {
    /// A document whose source Asset is a PDF.
    Pdf,
}
