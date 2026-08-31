//! Pure construction of collaboration descriptors from neutral source pages.

use orbitrelay_asset::SourceAssetDescriptor;
use orbitrelay_canvas::{CanvasDescriptor, CanvasId, CanvasSpace, LayerId};
use orbitrelay_document::{
    DocumentDescriptor, DocumentId, DocumentPageDescriptor, DocumentType, PageDisplayGeometry,
    PageId,
};
use orbitrelay_protocol::SessionId;

use crate::DocumentCompositionError;

/// A neutral source page supplied by a document-format adapter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DocumentSourcePage {
    page_index: u32,
    display_geometry: PageDisplayGeometry,
}

impl DocumentSourcePage {
    /// Creates one source page with its normalized display geometry.
    #[must_use]
    pub const fn new(page_index: u32, display_geometry: PageDisplayGeometry) -> Self {
        Self {
            page_index,
            display_geometry,
        }
    }

    /// Returns the source adapter's zero-based page index.
    #[must_use]
    pub const fn page_index(&self) -> u32 {
        self.page_index
    }

    /// Returns normalized displayed page geometry.
    #[must_use]
    pub const fn display_geometry(&self) -> &PageDisplayGeometry {
        &self.display_geometry
    }
}

/// Neutral input to [`DocumentComposer`].
///
/// The source title is an adapter-provided display hint. It is normalized at
/// construction and never mutates the immutable Asset descriptor.
#[derive(Clone, Debug, PartialEq)]
pub struct DocumentComposeInput {
    session_id: SessionId,
    document_type: DocumentType,
    source_asset: SourceAssetDescriptor,
    source_title: Option<String>,
    pages: Vec<DocumentSourcePage>,
}

impl DocumentComposeInput {
    /// Validates and creates a neutral composition input.
    pub fn new(
        session_id: SessionId,
        document_type: DocumentType,
        source_asset: SourceAssetDescriptor,
        source_title: Option<String>,
        pages: Vec<DocumentSourcePage>,
    ) -> Result<Self, DocumentCompositionError> {
        if pages.is_empty() {
            return Err(DocumentCompositionError::InvalidInput {
                reason: "at least one source page is required",
            });
        }

        for (position, page) in pages.iter().enumerate() {
            let expected =
                u32::try_from(position).map_err(|_| DocumentCompositionError::InvalidInput {
                    reason: "source page count exceeds u32 capacity",
                })?;
            if page.page_index != expected {
                return Err(DocumentCompositionError::InvalidPageSequence {
                    expected,
                    actual: page.page_index,
                });
            }
        }

        let source_title = source_title.and_then(|title| {
            let trimmed = title.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        });

        Ok(Self {
            session_id,
            document_type,
            source_asset,
            source_title,
            pages,
        })
    }

    /// Returns the target collaboration Session.
    #[must_use]
    pub const fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    /// Returns the neutral semantic document type.
    #[must_use]
    pub const fn document_type(&self) -> DocumentType {
        self.document_type
    }

    /// Returns the immutable source Asset metadata.
    #[must_use]
    pub const fn source_asset(&self) -> &SourceAssetDescriptor {
        &self.source_asset
    }

    /// Returns the normalized adapter-provided title hint.
    #[must_use]
    pub fn source_title(&self) -> Option<&str> {
        self.source_title.as_deref()
    }

    /// Returns source pages in their validated order.
    #[must_use]
    pub fn pages(&self) -> &[DocumentSourcePage] {
        &self.pages
    }
}

/// Returns the stable development fallback title for a supported type.
#[must_use]
pub const fn default_title_for(document_type: DocumentType) -> &'static str {
    match document_type {
        DocumentType::Pdf => "Untitled PDF",
    }
}

/// One page's generated collaboration Page and Canvas descriptors.
#[derive(Clone, Debug, PartialEq)]
pub struct PageCanvasComposition {
    page_id: PageId,
    canvas: CanvasDescriptor,
}

impl PageCanvasComposition {
    /// Creates a page-to-Canvas composition entry.
    #[must_use]
    pub const fn new(page_id: PageId, canvas: CanvasDescriptor) -> Self {
        Self { page_id, canvas }
    }

    /// Returns the generated Page identity.
    #[must_use]
    pub const fn page_id(&self) -> &PageId {
        &self.page_id
    }

    /// Returns the independent Canvas descriptor for this Page.
    #[must_use]
    pub const fn canvas(&self) -> &CanvasDescriptor {
        &self.canvas
    }
}

/// A complete immutable Document composition bundle.
#[derive(Clone, Debug, PartialEq)]
pub struct DocumentComposition {
    document: DocumentDescriptor,
    source_asset: SourceAssetDescriptor,
    page_canvases: Vec<PageCanvasComposition>,
}

impl DocumentComposition {
    /// Creates a bundle after validating all cross-domain invariants.
    pub fn new(
        document: DocumentDescriptor,
        source_asset: SourceAssetDescriptor,
        page_canvases: Vec<PageCanvasComposition>,
    ) -> Result<Self, DocumentCompositionError> {
        let composition = Self {
            document,
            source_asset,
            page_canvases,
        };
        composition.validate()?;
        Ok(composition)
    }

    /// Returns the generated Document descriptor.
    #[must_use]
    pub const fn document(&self) -> &DocumentDescriptor {
        &self.document
    }

    /// Returns the immutable source Asset metadata used by the Document.
    #[must_use]
    pub const fn source_asset(&self) -> &SourceAssetDescriptor {
        &self.source_asset
    }

    /// Returns one Canvas composition per Document page, in page order.
    #[must_use]
    pub fn page_canvases(&self) -> &[PageCanvasComposition] {
        &self.page_canvases
    }

    fn validate(&self) -> Result<(), DocumentCompositionError> {
        if self.document.source_asset_id() != self.source_asset.asset_id() {
            return Err(DocumentCompositionError::CompositionInvariantViolation {
                reason: "Document source Asset does not match composition metadata",
            });
        }
        if self.document.pages().len() != self.page_canvases.len() {
            return Err(DocumentCompositionError::CompositionInvariantViolation {
                reason: "Document page count does not match Canvas count",
            });
        }

        for (page, page_canvas) in self.document.pages().iter().zip(&self.page_canvases) {
            if page.page_id() != page_canvas.page_id() {
                return Err(DocumentCompositionError::CompositionInvariantViolation {
                    reason: "Page identity does not match its Canvas composition entry",
                });
            }
            let canvas = page_canvas.canvas();
            if page.overlay_canvas_id() != canvas.canvas_id() {
                return Err(DocumentCompositionError::CompositionInvariantViolation {
                    reason: "Page overlay Canvas does not match Canvas descriptor",
                });
            }
            if self.document.session_id() != canvas.session_id() {
                return Err(DocumentCompositionError::CompositionInvariantViolation {
                    reason: "Canvas belongs to a different Session",
                });
            }
            if page.display_geometry().width() != canvas.space().width()
                || page.display_geometry().height() != canvas.space().height()
            {
                return Err(DocumentCompositionError::CompositionInvariantViolation {
                    reason: "Page geometry does not match Canvas space",
                });
            }
            if !canvas.contains_layer(canvas.default_layer_id()) {
                return Err(DocumentCompositionError::CompositionInvariantViolation {
                    reason: "Canvas default layer is not in its layer set",
                });
            }
        }

        Ok(())
    }
}

/// Stateless pure service that creates collaboration identities and
/// descriptors from a neutral source input.
#[derive(Clone, Copy, Debug, Default)]
pub struct DocumentComposer;

impl DocumentComposer {
    /// Creates a stateless Composer.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Generates a complete immutable Document/Canvas composition.
    pub fn compose(
        &self,
        input: DocumentComposeInput,
    ) -> Result<DocumentComposition, DocumentCompositionError> {
        let DocumentComposeInput {
            session_id,
            document_type,
            source_asset,
            source_title,
            pages,
        } = input;

        let title = source_title
            .or_else(|| {
                source_asset
                    .original_filename()
                    .map(str::trim)
                    .filter(|filename| !filename.is_empty())
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| default_title_for(document_type).to_owned());

        let mut document_pages = Vec::with_capacity(pages.len());
        let mut page_canvases = Vec::with_capacity(pages.len());

        for page in pages {
            let page_id = PageId::new();
            let canvas_id = CanvasId::new();
            let layer_id = LayerId::new();
            let geometry = page.display_geometry;
            let space = CanvasSpace::new(geometry.width(), geometry.height())
                .map_err(|source| DocumentCompositionError::CanvasDescriptorFailed { source })?;
            let canvas = CanvasDescriptor::new(
                canvas_id.clone(),
                session_id.clone(),
                space,
                [layer_id.clone()],
                layer_id,
            )
            .map_err(|source| DocumentCompositionError::CanvasDescriptorFailed { source })?;

            document_pages.push(DocumentPageDescriptor::new(
                page_id.clone(),
                page.page_index,
                geometry,
                canvas_id,
            ));
            page_canvases.push(PageCanvasComposition::new(page_id, canvas));
        }

        let document = DocumentDescriptor::new(
            DocumentId::new(),
            session_id,
            document_type,
            source_asset.asset_id().clone(),
            title,
            document_pages,
        )
        .map_err(|source| DocumentCompositionError::DocumentDescriptorFailed { source })?;

        DocumentComposition::new(document, source_asset, page_canvases)
    }
}

#[cfg(test)]
mod tests {
    use orbitrelay_asset::{AssetId, ContentHash, SourceAssetDescriptor};
    use orbitrelay_document::{DocumentType, PageDisplayGeometry, PageRotation};
    use orbitrelay_protocol::SessionId;

    use super::{DocumentComposeInput, DocumentComposer, DocumentSourcePage};

    fn asset(filename: Option<&str>) -> SourceAssetDescriptor {
        SourceAssetDescriptor::new(
            AssetId::new(),
            "application/pdf",
            1,
            ContentHash::from_bytes([0x11; 32]),
            filename.map(str::to_owned),
        )
        .expect("asset metadata should be valid")
    }

    fn page(index: u32, width: f64, height: f64, rotation: PageRotation) -> DocumentSourcePage {
        DocumentSourcePage::new(
            index,
            PageDisplayGeometry::new(width, height, rotation).expect("geometry should be valid"),
        )
    }

    fn input(
        session_id: SessionId,
        title: Option<String>,
        filename: Option<&str>,
    ) -> DocumentComposeInput {
        DocumentComposeInput::new(
            session_id,
            DocumentType::Pdf,
            asset(filename),
            title,
            vec![page(0, 792.0, 612.0, PageRotation::Deg90)],
        )
        .expect("input should be valid")
    }

    #[test]
    fn composes_pages_and_canvas_without_second_rotation() {
        let composition = DocumentComposer::new()
            .compose(input(SessionId::new(), None, Some("lesson.pdf")))
            .expect("composition should be valid");
        let page = &composition.document().pages()[0];
        let canvas = composition.page_canvases()[0].canvas();

        assert_eq!(page.page_index(), 0);
        assert_eq!(page.display_geometry().width(), 792.0);
        assert_eq!(page.display_geometry().height(), 612.0);
        assert_eq!(canvas.space().width(), 792.0);
        assert_eq!(canvas.space().height(), 612.0);
        assert_eq!(canvas.layer_ids().len(), 1);
        assert!(canvas.contains_layer(canvas.default_layer_id()));
    }

    #[test]
    fn generates_distinct_collaboration_ids_for_same_asset_in_two_sessions() {
        let shared = asset(None);
        let make = |session_id| {
            DocumentComposeInput::new(
                session_id,
                DocumentType::Pdf,
                shared.clone(),
                None,
                vec![page(0, 10.0, 20.0, PageRotation::Deg0)],
            )
            .expect("input")
        };
        let first = DocumentComposer::new()
            .compose(make(SessionId::new()))
            .expect("first");
        let second = DocumentComposer::new()
            .compose(make(SessionId::new()))
            .expect("second");

        assert_eq!(
            first.document().source_asset_id(),
            second.document().source_asset_id()
        );
        assert_ne!(
            first.document().document_id(),
            second.document().document_id()
        );
        assert_ne!(
            first.document().pages()[0].page_id(),
            second.document().pages()[0].page_id()
        );
        assert_ne!(
            first.page_canvases()[0].canvas().canvas_id(),
            second.page_canvases()[0].canvas().canvas_id()
        );
        assert_ne!(
            first.page_canvases()[0].canvas().default_layer_id(),
            second.page_canvases()[0].canvas().default_layer_id()
        );
    }

    #[test]
    fn normalizes_title_and_applies_fallbacks() {
        let composer = DocumentComposer::new();
        let source = composer
            .compose(input(
                SessionId::new(),
                Some("  Source title  ".into()),
                Some("file.pdf"),
            ))
            .expect("source title")
            .document()
            .title()
            .to_owned();
        let filename = composer
            .compose(input(SessionId::new(), None, Some("  file.pdf  ")))
            .expect("filename")
            .document()
            .title()
            .to_owned();
        let fallback = composer
            .compose(input(SessionId::new(), Some("   ".into()), None))
            .expect("fallback")
            .document()
            .title()
            .to_owned();

        assert_eq!(source, "Source title");
        assert_eq!(filename, "file.pdf");
        assert_eq!(fallback, "Untitled PDF");
    }

    #[test]
    fn rejects_empty_or_noncontiguous_input_pages() {
        assert!(DocumentComposeInput::new(
            SessionId::new(),
            DocumentType::Pdf,
            asset(None),
            None,
            vec![],
        )
        .is_err());
        assert!(DocumentComposeInput::new(
            SessionId::new(),
            DocumentType::Pdf,
            asset(None),
            None,
            vec![page(1, 10.0, 10.0, PageRotation::Deg0)],
        )
        .is_err());
    }
}
