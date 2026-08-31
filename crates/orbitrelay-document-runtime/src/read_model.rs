//! Document discovery read models and cross-catalog assembly.

use std::sync::Arc;

use orbitrelay_asset::{AssetId, SourceAssetDescriptor};
use orbitrelay_asset_runtime::AssetCatalog;
use orbitrelay_canvas::CanvasDescriptor;
use orbitrelay_canvas_runtime::CanvasCatalog;
use orbitrelay_document::{DocumentDescriptor, DocumentId, DocumentType, PageId};

use crate::{DocumentCatalog, DocumentReadError};

/// Lightweight query projection for listing Documents.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentSummary {
    document_id: DocumentId,
    title: String,
    document_type: DocumentType,
    page_count: u32,
    source_asset_id: AssetId,
}

impl DocumentSummary {
    /// Builds a summary from a validated Document descriptor.
    pub fn from_document(document: &DocumentDescriptor) -> Result<Self, DocumentReadError> {
        let page_count = u32::try_from(document.pages().len())
            .map_err(|_| DocumentReadError::PageCountOverflow)?;
        Ok(Self {
            document_id: document.document_id().clone(),
            title: document.title().to_owned(),
            document_type: document.document_type(),
            page_count,
            source_asset_id: document.source_asset_id().clone(),
        })
    }

    /// Returns the Document identity.
    #[must_use]
    pub const fn document_id(&self) -> &DocumentId {
        &self.document_id
    }

    /// Returns the display title.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns the semantic Document type.
    #[must_use]
    pub const fn document_type(&self) -> DocumentType {
        self.document_type
    }

    /// Returns the number of ordered pages.
    #[must_use]
    pub const fn page_count(&self) -> u32 {
        self.page_count
    }

    /// Returns the immutable source Asset identity.
    #[must_use]
    pub const fn source_asset_id(&self) -> &AssetId {
        &self.source_asset_id
    }
}

/// One ordered Page-to-Canvas read projection.
#[derive(Clone, Debug, PartialEq)]
pub struct PageCanvasView {
    page_id: PageId,
    canvas: CanvasDescriptor,
}

impl PageCanvasView {
    /// Creates a Page/Canvas view entry.
    #[must_use]
    pub const fn new(page_id: PageId, canvas: CanvasDescriptor) -> Self {
        Self { page_id, canvas }
    }

    /// Returns the Document Page identity.
    #[must_use]
    pub const fn page_id(&self) -> &PageId {
        &self.page_id
    }

    /// Returns the corresponding Canvas descriptor.
    #[must_use]
    pub const fn canvas(&self) -> &CanvasDescriptor {
        &self.canvas
    }
}

/// Complete discovery read model for one Document.
#[derive(Clone, Debug, PartialEq)]
pub struct DocumentView {
    document: DocumentDescriptor,
    source_asset: SourceAssetDescriptor,
    page_canvases: Vec<PageCanvasView>,
}

impl DocumentView {
    /// Creates a read view from trusted, already cross-validated values.
    #[must_use]
    pub const fn new(
        document: DocumentDescriptor,
        source_asset: SourceAssetDescriptor,
        page_canvases: Vec<PageCanvasView>,
    ) -> Self {
        Self {
            document,
            source_asset,
            page_canvases,
        }
    }

    /// Returns the immutable Document descriptor.
    #[must_use]
    pub const fn document(&self) -> &DocumentDescriptor {
        &self.document
    }

    /// Returns source Asset metadata, without bytes or a download locator.
    #[must_use]
    pub const fn source_asset(&self) -> &SourceAssetDescriptor {
        &self.source_asset
    }

    /// Returns Page/Canvas entries in Document page order.
    #[must_use]
    pub fn page_canvases(&self) -> &[PageCanvasView] {
        &self.page_canvases
    }
}

/// Read service that assembles a complete Document view from independent
/// Document, Asset, and Canvas catalogs.
pub struct DocumentReadService {
    document_catalog: Arc<dyn DocumentCatalog>,
    asset_catalog: Arc<dyn AssetCatalog>,
    canvas_catalog: Arc<dyn CanvasCatalog>,
}

impl DocumentReadService {
    /// Creates a read service from independent read-only catalog ports.
    #[must_use]
    pub fn new(
        document_catalog: Arc<dyn DocumentCatalog>,
        asset_catalog: Arc<dyn AssetCatalog>,
        canvas_catalog: Arc<dyn CanvasCatalog>,
    ) -> Self {
        Self {
            document_catalog,
            asset_catalog,
            canvas_catalog,
        }
    }

    /// Reads a complete trusted view, or fails without returning partial data.
    pub async fn get_document_view(
        &self,
        document_id: &DocumentId,
    ) -> Result<DocumentView, DocumentReadError> {
        let document = self
            .document_catalog
            .get_document(document_id)
            .await
            .map_err(|_| DocumentReadError::CatalogUnavailable {
                catalog: "document",
            })?
            .ok_or_else(|| DocumentReadError::DocumentNotFound {
                document_id: document_id.clone(),
            })?;

        let source_asset = self
            .asset_catalog
            .get_asset(document.source_asset_id())
            .await
            .map_err(|_| DocumentReadError::CatalogUnavailable { catalog: "asset" })?
            .ok_or_else(|| DocumentReadError::AssetNotFound {
                asset_id: document.source_asset_id().clone(),
            })?;

        if source_asset.asset_id() != document.source_asset_id() {
            return Err(DocumentReadError::InconsistentReadModel {
                reason: "Asset catalog returned a different Asset identity",
            });
        }

        let mut page_canvases = Vec::with_capacity(document.pages().len());
        for page in document.pages() {
            let canvas_id = page.overlay_canvas_id();
            let canvas = self
                .canvas_catalog
                .get_canvas(canvas_id)
                .await
                .map_err(|_| DocumentReadError::CatalogUnavailable { catalog: "canvas" })?
                .ok_or_else(|| DocumentReadError::CanvasNotFound {
                    canvas_id: canvas_id.clone(),
                })?;

            validate_page_canvas(&document, page, &canvas)?;
            page_canvases.push(PageCanvasView::new(page.page_id().clone(), canvas));
        }

        if page_canvases.len() != document.pages().len() {
            return Err(DocumentReadError::InconsistentReadModel {
                reason: "Document page count does not match read model Canvas count",
            });
        }

        Ok(DocumentView::new(document, source_asset, page_canvases))
    }
}

fn validate_page_canvas(
    document: &DocumentDescriptor,
    page: &orbitrelay_document::DocumentPageDescriptor,
    canvas: &CanvasDescriptor,
) -> Result<(), DocumentReadError> {
    if page.overlay_canvas_id() != canvas.canvas_id() {
        return Err(DocumentReadError::InconsistentReadModel {
            reason: "Canvas identity does not match page overlay reference",
        });
    }
    if canvas.session_id() != document.session_id() {
        return Err(DocumentReadError::InconsistentReadModel {
            reason: "Canvas belongs to a different Session",
        });
    }
    if canvas.space().width() != page.display_geometry().width()
        || canvas.space().height() != page.display_geometry().height()
    {
        return Err(DocumentReadError::InconsistentReadModel {
            reason: "Canvas space does not match page display geometry",
        });
    }
    if !canvas.contains_layer(canvas.default_layer_id()) {
        return Err(DocumentReadError::InconsistentReadModel {
            reason: "Canvas default layer is not in its layer set",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use async_trait::async_trait;
    use orbitrelay_asset::{AssetId, ContentHash, SourceAssetDescriptor};
    use orbitrelay_asset_runtime::{AssetCatalog, AssetCatalogError};
    use orbitrelay_canvas::{CanvasDescriptor, CanvasId, CanvasSpace, LayerId};
    use orbitrelay_canvas_runtime::{CanvasCatalog, CanvasCatalogError};
    use orbitrelay_document::{
        DocumentDescriptor, DocumentType, PageDisplayGeometry, PageRotation,
    };
    use orbitrelay_protocol::SessionId;

    use super::DocumentReadService;
    use crate::{
        DocumentCatalog, DocumentCatalogError, DocumentComposeInput, DocumentComposer,
        DocumentSourcePage, MemoryDocumentCatalog,
    };

    #[derive(Default)]
    struct TestCanvasCatalog {
        canvases: HashMap<CanvasId, CanvasDescriptor>,
        unavailable: bool,
    }

    #[async_trait]
    impl CanvasCatalog for TestCanvasCatalog {
        async fn get_canvas(
            &self,
            canvas_id: &CanvasId,
        ) -> Result<Option<CanvasDescriptor>, CanvasCatalogError> {
            if self.unavailable {
                return Err(CanvasCatalogError::new("test failure"));
            }
            Ok(self.canvases.get(canvas_id).cloned())
        }
    }

    #[derive(Clone)]
    struct TestAssetCatalog {
        asset: Option<SourceAssetDescriptor>,
        unavailable: bool,
    }

    #[async_trait]
    impl AssetCatalog for TestAssetCatalog {
        async fn get_asset(
            &self,
            asset_id: &AssetId,
        ) -> Result<Option<SourceAssetDescriptor>, AssetCatalogError> {
            if self.unavailable {
                return Err(AssetCatalogError::Unavailable {
                    detail: "test failure".into(),
                });
            }
            Ok(self
                .asset
                .as_ref()
                .filter(|asset| asset.asset_id() == asset_id)
                .cloned())
        }
    }

    fn build() -> (DocumentDescriptor, SourceAssetDescriptor, TestCanvasCatalog) {
        let asset = SourceAssetDescriptor::new(
            AssetId::new(),
            "application/pdf",
            1,
            ContentHash::from_bytes([0x33; 32]),
            Some("lesson.pdf".into()),
        )
        .expect("asset");
        let input = DocumentComposeInput::new(
            SessionId::new(),
            DocumentType::Pdf,
            asset.clone(),
            Some("Lesson".into()),
            vec![
                DocumentSourcePage::new(
                    0,
                    PageDisplayGeometry::new(10.0, 20.0, PageRotation::Deg0).expect("geometry"),
                ),
                DocumentSourcePage::new(
                    1,
                    PageDisplayGeometry::new(30.0, 40.0, PageRotation::Deg90).expect("geometry"),
                ),
            ],
        )
        .expect("input");
        let composition = DocumentComposer::new().compose(input).expect("composition");
        let mut canvas_catalog = TestCanvasCatalog::default();
        for entry in composition.page_canvases() {
            canvas_catalog
                .canvases
                .insert(entry.canvas().canvas_id().clone(), entry.canvas().clone());
        }
        (composition.document().clone(), asset, canvas_catalog)
    }

    fn service(
        document: DocumentDescriptor,
        asset: SourceAssetDescriptor,
        canvas_catalog: TestCanvasCatalog,
    ) -> DocumentReadService {
        let documents = MemoryDocumentCatalog::new();
        assert!(matches!(
            documents.insert(document),
            crate::DocumentInsertOutcome::Inserted
        ));
        DocumentReadService::new(
            Arc::new(documents),
            Arc::new(TestAssetCatalog {
                asset: Some(asset),
                unavailable: false,
            }),
            Arc::new(canvas_catalog),
        )
    }

    #[tokio::test]
    async fn assembles_document_asset_and_canvas_views_in_page_order() {
        let (document, asset, canvases) = build();
        let id = document.document_id().clone();
        let view = service(document.clone(), asset.clone(), canvases)
            .get_document_view(&id)
            .await
            .expect("view");
        assert_eq!(view.document(), &document);
        assert_eq!(view.source_asset(), &asset);
        assert_eq!(view.page_canvases().len(), 2);
        assert_eq!(
            view.page_canvases()[0].page_id(),
            document.pages()[0].page_id()
        );
        assert_eq!(
            view.page_canvases()[1].page_id(),
            document.pages()[1].page_id()
        );
    }

    #[tokio::test]
    async fn missing_or_inconsistent_references_fail_without_partial_view() {
        let (document, asset, mut canvases) = build();
        let id = document.document_id().clone();
        canvases
            .canvases
            .remove(document.pages()[1].overlay_canvas_id());
        let missing = service(document.clone(), asset.clone(), canvases)
            .get_document_view(&id)
            .await;
        assert!(matches!(
            missing,
            Err(crate::DocumentReadError::CanvasNotFound { .. })
        ));

        let (document, asset, mut canvases) = build();
        let original = canvases
            .canvases
            .get_mut(document.pages()[0].overlay_canvas_id())
            .expect("canvas");
        let wrong_space = CanvasSpace::new(999.0, original.space().height()).expect("space");
        let layer = LayerId::new();
        let replacement = CanvasDescriptor::new(
            original.canvas_id().clone(),
            original.session_id().clone(),
            wrong_space,
            [layer.clone()],
            layer,
        )
        .expect("replacement canvas");
        *original = replacement;
        let inconsistent = service(document.clone(), asset, canvases)
            .get_document_view(&document.document_id().clone())
            .await;
        assert!(matches!(
            inconsistent,
            Err(crate::DocumentReadError::InconsistentReadModel { .. })
        ));

        let (document, asset, mut canvases) = build();
        let canvas = canvases
            .canvases
            .get_mut(document.pages()[0].overlay_canvas_id())
            .expect("canvas");
        let replacement_session = SessionId::new();
        let layer = LayerId::new();
        *canvas = CanvasDescriptor::new(
            canvas.canvas_id().clone(),
            replacement_session,
            canvas.space().to_owned(),
            [layer.clone()],
            layer,
        )
        .expect("session-mismatched canvas");
        let session_mismatch = service(document.clone(), asset, canvases)
            .get_document_view(&document.document_id().clone())
            .await;
        assert!(matches!(
            session_mismatch,
            Err(crate::DocumentReadError::InconsistentReadModel { .. })
        ));

        let (document, asset, mut canvases) = build();
        let expected_id = document.pages()[0].overlay_canvas_id().clone();
        let original = canvases.canvases.remove(&expected_id).expect("canvas");
        let replacement_id = CanvasId::new();
        let replacement = CanvasDescriptor::new(
            replacement_id.clone(),
            original.session_id().clone(),
            original.space().to_owned(),
            original.layer_ids().iter().cloned(),
            original.default_layer_id().clone(),
        )
        .expect("different canvas identity");
        canvases.canvases.insert(expected_id, replacement);
        let id_mismatch = service(document.clone(), asset, canvases)
            .get_document_view(&document.document_id().clone())
            .await;
        assert!(matches!(
            id_mismatch,
            Err(crate::DocumentReadError::InconsistentReadModel { .. })
        ));
    }

    #[tokio::test]
    async fn missing_asset_is_not_reported_as_missing_document() {
        let (document, _asset, canvases) = build();
        let documents = MemoryDocumentCatalog::new();
        assert!(matches!(
            documents.insert(document.clone()),
            crate::DocumentInsertOutcome::Inserted
        ));
        let service = DocumentReadService::new(
            Arc::new(documents),
            Arc::new(TestAssetCatalog {
                asset: None,
                unavailable: false,
            }),
            Arc::new(canvases),
        );
        assert!(matches!(
            service.get_document_view(document.document_id()).await,
            Err(crate::DocumentReadError::AssetNotFound { .. })
        ));
    }

    #[tokio::test]
    async fn unknown_document_and_catalog_failure_are_distinct() {
        let (document, asset, canvases) = build();
        let service = service(document.clone(), asset, canvases);
        let unknown = service
            .get_document_view(&orbitrelay_document::DocumentId::new())
            .await;
        assert!(matches!(
            unknown,
            Err(crate::DocumentReadError::DocumentNotFound { .. })
        ));

        struct FailingDocuments;
        #[async_trait]
        impl DocumentCatalog for FailingDocuments {
            async fn get_document(
                &self,
                _id: &orbitrelay_document::DocumentId,
            ) -> Result<Option<DocumentDescriptor>, DocumentCatalogError> {
                Err(DocumentCatalogError::new("backend"))
            }

            async fn list_documents(
                &self,
                _id: &SessionId,
            ) -> Result<Vec<crate::DocumentSummary>, DocumentCatalogError> {
                Err(DocumentCatalogError::new("backend"))
            }
        }
        let failing = DocumentReadService::new(
            Arc::new(FailingDocuments),
            Arc::new(TestAssetCatalog {
                asset: None,
                unavailable: false,
            }),
            Arc::new(TestCanvasCatalog::default()),
        );
        assert!(matches!(
            failing.get_document_view(document.document_id()).await,
            Err(crate::DocumentReadError::CatalogUnavailable {
                catalog: "document"
            })
        ));
    }
}
