//! Library-neutral output values from PDF inspection.

use orbitrelay_asset::AssetId;
use orbitrelay_document::PageDisplayGeometry;

/// PDF metadata and normalized page descriptors derived from one Asset.
#[derive(Clone, Debug, PartialEq)]
pub struct PdfDocumentMetadata {
    asset_id: AssetId,
    title: Option<String>,
    pages: Vec<PdfPageMetadata>,
}

impl PdfDocumentMetadata {
    pub(crate) fn new(
        asset_id: AssetId,
        title: Option<String>,
        pages: Vec<PdfPageMetadata>,
    ) -> Self {
        Self {
            asset_id,
            title,
            pages,
        }
    }

    /// Returns the inspected source Asset identity.
    #[must_use]
    pub const fn asset_id(&self) -> &AssetId {
        &self.asset_id
    }

    /// Returns the optional PDF Info title after safe decoding and trimming.
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Returns normalized pages in PDF page-tree order.
    #[must_use]
    pub fn pages(&self) -> &[PdfPageMetadata] {
        &self.pages
    }

    /// Returns the number of inspected pages.
    #[must_use]
    pub const fn page_count(&self) -> usize {
        self.pages.len()
    }
}

/// Metadata for one PDF page in logical page-tree order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PdfPageMetadata {
    page_index: u32,
    display_geometry: PageDisplayGeometry,
}

impl PdfPageMetadata {
    pub(crate) const fn new(page_index: u32, display_geometry: PageDisplayGeometry) -> Self {
        Self {
            page_index,
            display_geometry,
        }
    }

    /// Returns the zero-based logical page index.
    #[must_use]
    pub const fn page_index(self) -> u32 {
        self.page_index
    }

    /// Returns the normalized displayed page geometry.
    #[must_use]
    pub const fn display_geometry(self) -> PageDisplayGeometry {
        self.display_geometry
    }
}
