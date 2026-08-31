//! Immutable Document and Page descriptors.

use std::collections::HashSet;

use orbitrelay_asset::AssetId;
use orbitrelay_canvas::CanvasId;
use orbitrelay_protocol::SessionId;
use serde::{Deserialize, Serialize};

use crate::{DocumentError, DocumentId, DocumentType, PageDisplayGeometry, PageId};

/// Describes one ordered PDF page and its independent Canvas overlay.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentPageDescriptor {
    page_id: PageId,
    page_index: u32,
    display_geometry: PageDisplayGeometry,
    overlay_canvas_id: CanvasId,
}

impl DocumentPageDescriptor {
    /// Creates a page descriptor. Document-level index and uniqueness checks
    /// are performed by [`DocumentDescriptor::new`].
    #[must_use]
    pub const fn new(
        page_id: PageId,
        page_index: u32,
        display_geometry: PageDisplayGeometry,
        overlay_canvas_id: CanvasId,
    ) -> Self {
        Self {
            page_id,
            page_index,
            display_geometry,
            overlay_canvas_id,
        }
    }

    /// Returns this page's stable identity.
    #[must_use]
    pub const fn page_id(&self) -> &PageId {
        &self.page_id
    }

    /// Returns this page's zero-based position within the Document.
    #[must_use]
    pub const fn page_index(&self) -> u32 {
        self.page_index
    }

    /// Returns the normalized visible page geometry.
    #[must_use]
    pub const fn display_geometry(&self) -> &PageDisplayGeometry {
        &self.display_geometry
    }

    /// Returns the independent Canvas overlay identity.
    #[must_use]
    pub const fn overlay_canvas_id(&self) -> &CanvasId {
        &self.overlay_canvas_id
    }
}

impl<'de> Deserialize<'de> for DocumentPageDescriptor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            page_id: PageId,
            page_index: u32,
            display_geometry: PageDisplayGeometry,
            overlay_canvas_id: CanvasId,
        }

        let fields = Fields::deserialize(deserializer)?;
        Ok(Self::new(
            fields.page_id,
            fields.page_index,
            fields.display_geometry,
            fields.overlay_canvas_id,
        ))
    }
}

/// Describes one static Document collaboration object.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentDescriptor {
    document_id: DocumentId,
    session_id: SessionId,
    document_type: DocumentType,
    source_asset_id: AssetId,
    title: String,
    pages: Vec<DocumentPageDescriptor>,
}

impl DocumentDescriptor {
    /// Creates a validated, ordered Document descriptor.
    pub fn new(
        document_id: DocumentId,
        session_id: SessionId,
        document_type: DocumentType,
        source_asset_id: AssetId,
        title: impl Into<String>,
        pages: Vec<DocumentPageDescriptor>,
    ) -> Result<Self, DocumentError> {
        let title = title.into();
        if title.trim().is_empty() {
            return Err(DocumentError::InvalidTitle);
        }
        if pages.is_empty() {
            return Err(DocumentError::EmptyPages);
        }

        let mut page_ids = HashSet::with_capacity(pages.len());
        let mut page_indexes = HashSet::with_capacity(pages.len());
        let mut canvas_ids = HashSet::with_capacity(pages.len());

        for (position, page) in pages.iter().enumerate() {
            let expected_index =
                u32::try_from(position).map_err(|_| DocumentError::InvalidDescriptor {
                    reason: "page vector is too large for a u32 page index",
                })?;
            if !page_ids.insert(page.page_id().clone()) {
                return Err(DocumentError::DuplicatePageId {
                    page_id: page.page_id().clone(),
                });
            }
            if !page_indexes.insert(page.page_index()) {
                return Err(DocumentError::DuplicatePageIndex {
                    page_index: page.page_index(),
                });
            }
            if page.page_index() != expected_index {
                return Err(DocumentError::NonContiguousPageIndex {
                    expected: expected_index,
                    actual: page.page_index(),
                });
            }
            if !canvas_ids.insert(page.overlay_canvas_id().clone()) {
                return Err(DocumentError::DuplicateCanvasId {
                    canvas_id: page.overlay_canvas_id().clone(),
                });
            }
        }

        Ok(Self {
            document_id,
            session_id,
            document_type,
            source_asset_id,
            title,
            pages,
        })
    }

    /// Returns this Document's stable identity.
    #[must_use]
    pub const fn document_id(&self) -> &DocumentId {
        &self.document_id
    }

    /// Returns the single Session that owns this Document.
    #[must_use]
    pub const fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    /// Returns this Document's semantic type.
    #[must_use]
    pub const fn document_type(&self) -> DocumentType {
        self.document_type
    }

    /// Returns the immutable source Asset identity.
    #[must_use]
    pub const fn source_asset_id(&self) -> &AssetId {
        &self.source_asset_id
    }

    /// Returns the display title metadata.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns pages in their validated zero-based order.
    #[must_use]
    pub fn pages(&self) -> &[DocumentPageDescriptor] {
        &self.pages
    }
}

impl<'de> Deserialize<'de> for DocumentDescriptor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            document_id: DocumentId,
            session_id: SessionId,
            document_type: DocumentType,
            source_asset_id: AssetId,
            title: String,
            pages: Vec<DocumentPageDescriptor>,
        }

        let fields = Fields::deserialize(deserializer)?;
        Self::new(
            fields.document_id,
            fields.session_id,
            fields.document_type,
            fields.source_asset_id,
            fields.title,
            fields.pages,
        )
        .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use orbitrelay_asset::AssetId;
    use orbitrelay_canvas::CanvasId;
    use orbitrelay_protocol::SessionId;

    use super::{DocumentDescriptor, DocumentPageDescriptor};
    use crate::{
        DocumentError, DocumentId, DocumentType, PageDisplayGeometry, PageId, PageRotation,
    };

    fn page(index: u32, page_id: PageId, canvas_id: CanvasId) -> DocumentPageDescriptor {
        DocumentPageDescriptor::new(
            page_id,
            index,
            PageDisplayGeometry::new(595.0, 842.0, PageRotation::Deg0).expect("geometry"),
            canvas_id,
        )
    }

    fn document(pages: Vec<DocumentPageDescriptor>) -> DocumentDescriptor {
        DocumentDescriptor::new(
            DocumentId::new(),
            SessionId::new(),
            DocumentType::Pdf,
            AssetId::new(),
            "Algebra",
            pages,
        )
        .expect("document should be valid")
    }

    #[test]
    fn accepts_single_and_multiple_ordered_pages() {
        let first = page(0, PageId::new(), CanvasId::new());
        let second = page(1, PageId::new(), CanvasId::new());
        let descriptor = document(vec![first, second]);

        assert_eq!(descriptor.pages().len(), 2);
        assert_eq!(descriptor.pages()[1].page_index(), 1);
    }

    #[test]
    fn rejects_empty_pages_blank_title_and_invalid_index_shapes() {
        let base = DocumentId::new();
        let session = SessionId::new();
        let asset = AssetId::new();
        assert!(matches!(
            DocumentDescriptor::new(
                base.clone(),
                session.clone(),
                DocumentType::Pdf,
                asset.clone(),
                "title",
                vec![]
            ),
            Err(DocumentError::EmptyPages)
        ));
        assert!(matches!(
            DocumentDescriptor::new(
                base,
                session,
                DocumentType::Pdf,
                asset,
                "  ",
                vec![page(0, PageId::new(), CanvasId::new())]
            ),
            Err(DocumentError::InvalidTitle)
        ));
        let invalid = vec![page(1, PageId::new(), CanvasId::new())];
        assert!(matches!(
            document_result(invalid),
            Err(DocumentError::NonContiguousPageIndex {
                expected: 0,
                actual: 1
            })
        ));
    }

    #[test]
    fn rejects_duplicate_page_and_canvas_ids() {
        let page_id = PageId::new();
        let canvas_id = CanvasId::new();
        assert!(matches!(
            document_result(vec![
                page(0, page_id.clone(), canvas_id.clone()),
                page(1, page_id, CanvasId::new())
            ]),
            Err(DocumentError::DuplicatePageId { .. })
        ));
        assert!(matches!(
            document_result(vec![
                page(0, PageId::new(), canvas_id.clone()),
                page(1, PageId::new(), canvas_id)
            ]),
            Err(DocumentError::DuplicateCanvasId { .. })
        ));
    }

    #[test]
    fn rejects_duplicate_indices_and_ordered_vec_mismatch() {
        assert!(matches!(
            document_result(vec![
                page(0, PageId::new(), CanvasId::new()),
                page(0, PageId::new(), CanvasId::new())
            ]),
            Err(DocumentError::DuplicatePageIndex { page_index: 0 })
        ));
        assert!(matches!(
            document_result(vec![
                page(1, PageId::new(), CanvasId::new()),
                page(0, PageId::new(), CanvasId::new())
            ]),
            Err(DocumentError::NonContiguousPageIndex {
                expected: 0,
                actual: 1
            })
        ));
    }

    #[test]
    fn same_asset_can_back_distinct_documents_and_sessions() {
        let asset_id = AssetId::new();
        let first = DocumentDescriptor::new(
            DocumentId::new(),
            SessionId::new(),
            DocumentType::Pdf,
            asset_id.clone(),
            "A",
            vec![page(0, PageId::new(), CanvasId::new())],
        )
        .expect("first document");
        let second = DocumentDescriptor::new(
            DocumentId::new(),
            SessionId::new(),
            DocumentType::Pdf,
            asset_id,
            "B",
            vec![page(0, PageId::new(), CanvasId::new())],
        )
        .expect("second document");

        assert_ne!(first.document_id(), second.document_id());
        assert_ne!(first.session_id(), second.session_id());
        assert_eq!(first.source_asset_id(), second.source_asset_id());
    }

    #[test]
    fn descriptor_round_trips_through_json() {
        let descriptor = document(vec![page(0, PageId::new(), CanvasId::new())]);
        let encoded = serde_json::to_string(&descriptor).expect("document should serialize");
        let decoded: DocumentDescriptor =
            serde_json::from_str(&encoded).expect("document should deserialize");
        assert_eq!(decoded, descriptor);
    }

    #[test]
    fn invalid_page_order_is_rejected_at_json_boundary() {
        let value = serde_json::json!({
            "document_id": DocumentId::new(),
            "session_id": SessionId::new(),
            "document_type": "pdf",
            "source_asset_id": AssetId::new(),
            "title": "Algebra",
            "pages": [{
                "page_id": PageId::new(),
                "page_index": 1,
                "display_geometry": {
                    "width": 595.0,
                    "height": 842.0,
                    "rotation": 0
                },
                "overlay_canvas_id": CanvasId::new()
            }]
        });

        assert!(serde_json::from_value::<DocumentDescriptor>(value).is_err());
    }

    fn document_result(
        pages: Vec<DocumentPageDescriptor>,
    ) -> Result<DocumentDescriptor, DocumentError> {
        DocumentDescriptor::new(
            DocumentId::new(),
            SessionId::new(),
            DocumentType::Pdf,
            AssetId::new(),
            "title",
            pages,
        )
    }
}
