//! Resource policy for PDF inspection.

/// Bounds applied by the PDF metadata adapter.
///
/// These are adapter/deployment policies, not invariants of the Asset or
/// Document domains. In particular, `max_asset_bytes` does not make a source
/// Asset invalid; it only limits one inspection request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PdfInspectionLimits {
    max_asset_bytes: u64,
    max_pages: u32,
    max_decompressed_stream_bytes: u64,
}

impl PdfInspectionLimits {
    /// Conservative development defaults: 64 MiB input, 4096 pages, and a
    /// 16 MiB limit for any one decompressed lopdf object/xref stream.
    pub const DEFAULT: Self = Self {
        max_asset_bytes: 64 * 1024 * 1024,
        max_pages: 4096,
        max_decompressed_stream_bytes: 16 * 1024 * 1024,
    };

    /// Creates an inspection policy with explicit byte, page, and stream
    /// limits.
    #[must_use]
    pub const fn new(
        max_asset_bytes: u64,
        max_pages: u32,
        max_decompressed_stream_bytes: u64,
    ) -> Self {
        Self {
            max_asset_bytes,
            max_pages,
            max_decompressed_stream_bytes,
        }
    }

    /// Returns the maximum Asset size accepted by one inspection.
    #[must_use]
    pub const fn max_asset_bytes(self) -> u64 {
        self.max_asset_bytes
    }

    /// Returns the maximum number of pages accepted by one inspection.
    #[must_use]
    pub const fn max_pages(self) -> u32 {
        self.max_pages
    }

    /// Returns the maximum output of one decompressed parser stream.
    #[must_use]
    pub const fn max_decompressed_stream_bytes(self) -> u64 {
        self.max_decompressed_stream_bytes
    }
}

impl Default for PdfInspectionLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}
