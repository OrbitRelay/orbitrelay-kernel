//! Typed Document Query handlers and explicit read DTOs.

use std::sync::Arc;

use async_trait::async_trait;
use orbitrelay_asset::{AssetId, SourceAssetDescriptor};
use orbitrelay_canvas::CanvasDescriptor;
use orbitrelay_document::{DocumentDescriptor, DocumentId, DocumentType, PageDisplayGeometry};
use orbitrelay_protocol::{Payload, SessionId};
use orbitrelay_query::{
    QueryActorContext, QueryHandler, QueryHandlerError, QueryRegistry, QueryRegistryError,
    QueryRequest, QueryType, QueryTypeError,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    DocumentCatalog, DocumentReadError, DocumentReadService, DocumentSummary, DocumentView,
};

/// Stable names for the first Document Query handlers.
pub const DOCUMENT_LIST_QUERY_TYPE: &str = "document.list";
/// Stable name for the Document detail Query handler.
pub const DOCUMENT_GET_QUERY_TYPE: &str = "document.get";

/// Authorization boundary for Document metadata reads.
#[async_trait]
pub trait DocumentReadAuthorizer: Send + Sync {
    /// Authorizes reading all Documents in a Session for a Query type.
    async fn authorize_session_read(
        &self,
        actor: &QueryActorContext,
        session_id: &SessionId,
        query_type: &QueryType,
    ) -> Result<(), DocumentReadAuthorizationError>;
}

/// Stable failures from a Document read authorizer.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum DocumentReadAuthorizationError {
    /// The actor is not allowed to read this Session.
    #[error("document read is unauthorized")]
    Unauthorized,
    /// The authorization dependency is unavailable.
    #[error("document read authorization is unavailable")]
    Unavailable,
    /// Authorization failed unexpectedly.
    #[error("document read authorization failed")]
    Internal,
}

/// Typed payload for `document.list`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListDocumentsPayload {
    session_id: SessionId,
}

/// Typed payload for `document.get`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GetDocumentPayload {
    document_id: DocumentId,
}

/// Explicit wire DTO for one listed Document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentDto {
    document_id: DocumentId,
    session_id: SessionId,
    document_type: DocumentType,
    source_asset_id: AssetId,
    title: String,
    pages: Vec<DocumentPageDto>,
}

impl DocumentDto {
    fn from_document(document: &DocumentDescriptor) -> Self {
        Self {
            document_id: document.document_id().clone(),
            session_id: document.session_id().clone(),
            document_type: document.document_type(),
            source_asset_id: document.source_asset_id().clone(),
            title: document.title().to_owned(),
            pages: document
                .pages()
                .iter()
                .map(DocumentPageDto::from_domain)
                .collect(),
        }
    }

    /// Returns the Document identifier.
    #[must_use]
    pub const fn document_id(&self) -> &DocumentId {
        &self.document_id
    }

    /// Returns the owning Session.
    #[must_use]
    pub const fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    /// Returns the semantic Document type.
    #[must_use]
    pub const fn document_type(&self) -> DocumentType {
        self.document_type
    }

    /// Returns the source Asset identifier.
    #[must_use]
    pub const fn source_asset_id(&self) -> &AssetId {
        &self.source_asset_id
    }

    /// Returns the display title.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns pages in logical Document order.
    #[must_use]
    pub fn pages(&self) -> &[DocumentPageDto] {
        &self.pages
    }
}

/// Explicit wire DTO for one Document summary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentSummaryDto {
    document_id: DocumentId,
    title: String,
    document_type: DocumentType,
    page_count: u32,
    source_asset_id: AssetId,
}

impl From<&DocumentSummary> for DocumentSummaryDto {
    fn from(summary: &DocumentSummary) -> Self {
        Self {
            document_id: summary.document_id().clone(),
            title: summary.title().to_owned(),
            document_type: summary.document_type(),
            page_count: summary.page_count(),
            source_asset_id: summary.source_asset_id().clone(),
        }
    }
}

impl DocumentSummaryDto {
    /// Returns the summarized Document identifier.
    #[must_use]
    pub const fn document_id(&self) -> &DocumentId {
        &self.document_id
    }

    /// Returns the summary title.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns the semantic Document type.
    #[must_use]
    pub const fn document_type(&self) -> DocumentType {
        self.document_type
    }

    /// Returns the number of pages represented by this summary.
    #[must_use]
    pub const fn page_count(&self) -> u32 {
        self.page_count
    }

    /// Returns the summarized source Asset identity.
    #[must_use]
    pub const fn source_asset_id(&self) -> &AssetId {
        &self.source_asset_id
    }
}

/// Explicit wire payload for `document.list` success.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentListResultDto {
    documents: Vec<DocumentSummaryDto>,
}

impl DocumentListResultDto {
    /// Creates a list result from runtime summaries.
    #[must_use]
    pub fn new(documents: Vec<DocumentSummaryDto>) -> Self {
        Self { documents }
    }

    /// Returns listed summaries in catalog order.
    #[must_use]
    pub fn documents(&self) -> &[DocumentSummaryDto] {
        &self.documents
    }
}

/// Explicit wire DTO for normalized page geometry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentPageDto {
    page_id: orbitrelay_document::PageId,
    page_index: u32,
    display_geometry: PageDisplayGeometry,
    overlay_canvas_id: orbitrelay_canvas::CanvasId,
}

impl DocumentPageDto {
    fn from_domain(page: &orbitrelay_document::DocumentPageDescriptor) -> Self {
        Self {
            page_id: page.page_id().clone(),
            page_index: page.page_index(),
            display_geometry: *page.display_geometry(),
            overlay_canvas_id: page.overlay_canvas_id().clone(),
        }
    }

    /// Returns the Page identity.
    #[must_use]
    pub const fn page_id(&self) -> &orbitrelay_document::PageId {
        &self.page_id
    }

    /// Returns the logical zero-based page index.
    #[must_use]
    pub const fn page_index(&self) -> u32 {
        self.page_index
    }

    /// Returns normalized displayed geometry.
    #[must_use]
    pub const fn display_geometry(&self) -> &PageDisplayGeometry {
        &self.display_geometry
    }

    /// Returns the overlay Canvas identity.
    #[must_use]
    pub const fn overlay_canvas_id(&self) -> &orbitrelay_canvas::CanvasId {
        &self.overlay_canvas_id
    }
}

/// Explicit wire DTO for source Asset metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceAssetDto {
    asset_id: AssetId,
    media_type: String,
    byte_length: u64,
    content_hash: orbitrelay_asset::ContentHash,
    original_filename: Option<String>,
}

impl From<&SourceAssetDescriptor> for SourceAssetDto {
    fn from(asset: &SourceAssetDescriptor) -> Self {
        Self {
            asset_id: asset.asset_id().clone(),
            media_type: asset.media_type().to_owned(),
            byte_length: asset.byte_length(),
            content_hash: asset.content_hash().clone(),
            original_filename: asset.original_filename().map(str::to_owned),
        }
    }
}

impl SourceAssetDto {
    /// Returns the immutable Asset identity.
    #[must_use]
    pub const fn asset_id(&self) -> &AssetId {
        &self.asset_id
    }

    /// Returns the source media type.
    #[must_use]
    pub fn media_type(&self) -> &str {
        &self.media_type
    }

    /// Returns the source byte length.
    #[must_use]
    pub const fn byte_length(&self) -> u64 {
        self.byte_length
    }

    /// Returns the source content hash.
    #[must_use]
    pub const fn content_hash(&self) -> &orbitrelay_asset::ContentHash {
        &self.content_hash
    }

    /// Returns the optional display filename.
    #[must_use]
    pub fn original_filename(&self) -> Option<&str> {
        self.original_filename.as_deref()
    }
}

/// Explicit wire DTO for Canvas logical space.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanvasSpaceDto {
    width: f64,
    height: f64,
}

impl CanvasSpaceDto {
    /// Returns the logical Canvas width.
    #[must_use]
    pub const fn width(&self) -> f64 {
        self.width
    }

    /// Returns the logical Canvas height.
    #[must_use]
    pub const fn height(&self) -> f64 {
        self.height
    }
}

/// Explicit wire DTO for one Page's Canvas descriptor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanvasDto {
    canvas_id: orbitrelay_canvas::CanvasId,
    session_id: SessionId,
    space: CanvasSpaceDto,
    layer_ids: Vec<orbitrelay_canvas::LayerId>,
    default_layer_id: orbitrelay_canvas::LayerId,
}

impl From<&CanvasDescriptor> for CanvasDto {
    fn from(canvas: &CanvasDescriptor) -> Self {
        Self {
            canvas_id: canvas.canvas_id().clone(),
            session_id: canvas.session_id().clone(),
            space: CanvasSpaceDto {
                width: canvas.space().width(),
                height: canvas.space().height(),
            },
            layer_ids: canvas.layer_ids().iter().cloned().collect(),
            default_layer_id: canvas.default_layer_id().clone(),
        }
    }
}

impl CanvasDto {
    /// Returns the Canvas identity.
    #[must_use]
    pub const fn canvas_id(&self) -> &orbitrelay_canvas::CanvasId {
        &self.canvas_id
    }

    /// Returns the owning Session.
    #[must_use]
    pub const fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    /// Returns the logical Canvas space.
    #[must_use]
    pub const fn space(&self) -> &CanvasSpaceDto {
        &self.space
    }

    /// Returns valid Layer identities in stable order.
    #[must_use]
    pub fn layer_ids(&self) -> &[orbitrelay_canvas::LayerId] {
        &self.layer_ids
    }

    /// Returns the default Layer identity.
    #[must_use]
    pub const fn default_layer_id(&self) -> &orbitrelay_canvas::LayerId {
        &self.default_layer_id
    }
}

/// Explicit wire DTO for one Page-to-Canvas mapping.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageCanvasDto {
    page_id: orbitrelay_document::PageId,
    canvas: CanvasDto,
}

impl From<(&orbitrelay_document::PageId, &CanvasDescriptor)> for PageCanvasDto {
    fn from((page_id, canvas): (&orbitrelay_document::PageId, &CanvasDescriptor)) -> Self {
        Self {
            page_id: page_id.clone(),
            canvas: canvas.into(),
        }
    }
}

impl PageCanvasDto {
    /// Returns the mapped Page identity.
    #[must_use]
    pub const fn page_id(&self) -> &orbitrelay_document::PageId {
        &self.page_id
    }

    /// Returns the mapped Canvas descriptor.
    #[must_use]
    pub const fn canvas(&self) -> &CanvasDto {
        &self.canvas
    }
}

/// Explicit wire payload for `document.get` success.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentViewDto {
    document: DocumentDto,
    source_asset: SourceAssetDto,
    page_canvases: Vec<PageCanvasDto>,
}

impl From<&DocumentView> for DocumentViewDto {
    fn from(view: &DocumentView) -> Self {
        Self {
            document: DocumentDto::from_document(view.document()),
            source_asset: view.source_asset().into(),
            page_canvases: view
                .page_canvases()
                .iter()
                .map(|entry| (entry.page_id(), entry.canvas()).into())
                .collect(),
        }
    }
}

impl DocumentViewDto {
    /// Returns the nested Document DTO.
    #[must_use]
    pub const fn document(&self) -> &DocumentDto {
        &self.document
    }

    /// Returns source Asset metadata.
    #[must_use]
    pub const fn source_asset(&self) -> &SourceAssetDto {
        &self.source_asset
    }

    /// Returns Page/Canvas mappings in Document order.
    #[must_use]
    pub fn page_canvases(&self) -> &[PageCanvasDto] {
        &self.page_canvases
    }
}

/// Handles `document.list` after typed payload validation and authorization.
pub struct DocumentListQueryHandler {
    query_type: QueryType,
    catalog: Arc<dyn DocumentCatalog>,
    authorizer: Arc<dyn DocumentReadAuthorizer>,
}

impl DocumentListQueryHandler {
    /// Creates a list handler over the supplied catalog and authorizer.
    pub fn new(
        catalog: Arc<dyn DocumentCatalog>,
        authorizer: Arc<dyn DocumentReadAuthorizer>,
    ) -> Result<Self, QueryTypeError> {
        Ok(Self {
            query_type: QueryType::new(DOCUMENT_LIST_QUERY_TYPE)?,
            catalog,
            authorizer,
        })
    }
}

/// Handles `document.get` with resolve-then-authorize ordering.
pub struct DocumentGetQueryHandler {
    query_type: QueryType,
    catalog: Arc<dyn DocumentCatalog>,
    read_service: Arc<DocumentReadService>,
    authorizer: Arc<dyn DocumentReadAuthorizer>,
}

impl DocumentGetQueryHandler {
    /// Creates a get handler over independent catalogs and read authorizer.
    pub fn new(
        catalog: Arc<dyn DocumentCatalog>,
        read_service: Arc<DocumentReadService>,
        authorizer: Arc<dyn DocumentReadAuthorizer>,
    ) -> Result<Self, QueryTypeError> {
        Ok(Self {
            query_type: QueryType::new(DOCUMENT_GET_QUERY_TYPE)?,
            catalog,
            read_service,
            authorizer,
        })
    }
}

#[async_trait]
impl QueryHandler for DocumentListQueryHandler {
    fn query_type(&self) -> &QueryType {
        &self.query_type
    }

    async fn execute(
        &self,
        actor: &QueryActorContext,
        request: QueryRequest,
    ) -> Result<Payload, QueryHandlerError> {
        let payload: ListDocumentsPayload = decode_payload(request.payload())?;
        self.authorizer
            .authorize_session_read(actor, &payload.session_id, &self.query_type)
            .await
            .map_err(map_authorization_error)?;
        let summaries = self
            .catalog
            .list_documents(&payload.session_id)
            .await
            .map_err(|_| QueryHandlerError::Unavailable)?;
        encode_payload(&DocumentListResultDto::new(
            summaries.iter().map(Into::into).collect(),
        ))
    }
}

#[async_trait]
impl QueryHandler for DocumentGetQueryHandler {
    fn query_type(&self) -> &QueryType {
        &self.query_type
    }

    async fn execute(
        &self,
        actor: &QueryActorContext,
        request: QueryRequest,
    ) -> Result<Payload, QueryHandlerError> {
        let payload: GetDocumentPayload = decode_payload(request.payload())?;
        let document = self
            .catalog
            .get_document(&payload.document_id)
            .await
            .map_err(|_| QueryHandlerError::Unavailable)?
            .ok_or(QueryHandlerError::NotFound)?;
        self.authorizer
            .authorize_session_read(actor, document.session_id(), &self.query_type)
            .await
            .map_err(map_authorization_error)?;
        let view = self
            .read_service
            .get_document_view(&payload.document_id)
            .await
            .map_err(map_read_error)?;
        encode_payload(&DocumentViewDto::from(&view))
    }
}

/// Registers the two first-party Document handlers into a generic registry.
pub fn register_document_query_handlers(
    registry: &mut QueryRegistry,
    catalog: Arc<dyn DocumentCatalog>,
    read_service: Arc<DocumentReadService>,
    authorizer: Arc<dyn DocumentReadAuthorizer>,
) -> Result<(), QueryRegistryError> {
    let list = DocumentListQueryHandler::new(Arc::clone(&catalog), Arc::clone(&authorizer))
        .map_err(|_| QueryRegistryError::DuplicateQueryType {
            query_type: QueryType::new(DOCUMENT_LIST_QUERY_TYPE)
                .expect("static query type should remain valid"),
        })?;
    let get = DocumentGetQueryHandler::new(catalog, read_service, authorizer).map_err(|_| {
        QueryRegistryError::DuplicateQueryType {
            query_type: QueryType::new(DOCUMENT_GET_QUERY_TYPE)
                .expect("static query type should remain valid"),
        }
    })?;
    registry.register(Arc::new(list))?;
    registry.register(Arc::new(get))?;
    Ok(())
}

fn decode_payload<T: for<'de> Deserialize<'de>>(payload: &Payload) -> Result<T, QueryHandlerError> {
    let value = serde_json::to_value(payload).map_err(|_| QueryHandlerError::InvalidQuery)?;
    serde_json::from_value(value).map_err(|_| QueryHandlerError::InvalidQuery)
}

fn encode_payload<T: Serialize>(value: &T) -> Result<Payload, QueryHandlerError> {
    let value = serde_json::to_value(value).map_err(|_| QueryHandlerError::Internal)?;
    serde_json::from_value(value).map_err(|_| QueryHandlerError::Internal)
}

fn map_authorization_error(error: DocumentReadAuthorizationError) -> QueryHandlerError {
    match error {
        DocumentReadAuthorizationError::Unauthorized => QueryHandlerError::Unauthorized,
        DocumentReadAuthorizationError::Unavailable => QueryHandlerError::Unavailable,
        DocumentReadAuthorizationError::Internal => QueryHandlerError::Internal,
    }
}

fn map_read_error(error: DocumentReadError) -> QueryHandlerError {
    match error {
        DocumentReadError::DocumentNotFound { .. } => QueryHandlerError::NotFound,
        DocumentReadError::AssetNotFound { .. }
        | DocumentReadError::CanvasNotFound { .. }
        | DocumentReadError::InconsistentReadModel { .. }
        | DocumentReadError::PageCountOverflow => QueryHandlerError::Internal,
        DocumentReadError::CatalogUnavailable { .. } => QueryHandlerError::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    };

    use async_trait::async_trait;
    use orbitrelay_asset::{AssetId, ContentHash, SourceAssetDescriptor};
    use orbitrelay_asset_runtime::{AssetCatalog, AssetCatalogError};
    use orbitrelay_canvas::{CanvasDescriptor, CanvasId};
    use orbitrelay_canvas_runtime::{CanvasCatalog, CanvasCatalogError};
    use orbitrelay_document::{
        DocumentDescriptor, DocumentType, PageDisplayGeometry, PageRotation,
    };
    use orbitrelay_protocol::{ActorId, SessionId};
    use orbitrelay_query::{
        QueryActorContext, QueryExecutor, QueryFailureCode, QueryRegistry, QueryRequest,
        QueryResult, QueryType, RegisteredQueryExecutor,
    };
    use serde_json::json;

    use super::{
        DocumentGetQueryHandler, DocumentListQueryHandler, DocumentReadAuthorizationError,
        DocumentReadAuthorizer, DocumentViewDto,
    };
    use crate::{
        DocumentCatalog, DocumentCatalogError, DocumentComposeInput, DocumentComposer,
        DocumentReadService, DocumentSourcePage, DocumentSummary, MemoryDocumentCatalog,
    };

    struct TestAssetCatalog {
        asset: Option<SourceAssetDescriptor>,
        calls: Arc<AtomicUsize>,
        unavailable: bool,
    }

    #[async_trait]
    impl AssetCatalog for TestAssetCatalog {
        async fn get_asset(
            &self,
            _asset_id: &AssetId,
        ) -> Result<Option<SourceAssetDescriptor>, AssetCatalogError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.unavailable {
                return Err(AssetCatalogError::Unavailable {
                    detail: "test backend".to_owned(),
                });
            }
            Ok(self.asset.clone())
        }
    }

    struct TestCanvasCatalog {
        canvases: HashMap<CanvasId, CanvasDescriptor>,
        calls: Arc<AtomicUsize>,
        unavailable: bool,
    }

    #[async_trait]
    impl CanvasCatalog for TestCanvasCatalog {
        async fn get_canvas(
            &self,
            canvas_id: &CanvasId,
        ) -> Result<Option<CanvasDescriptor>, CanvasCatalogError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.unavailable {
                return Err(CanvasCatalogError::new("test backend"));
            }
            Ok(self.canvases.get(canvas_id).cloned())
        }
    }

    struct TestAuthorizer {
        allow: bool,
        calls: Arc<Mutex<Vec<SessionId>>>,
    }

    #[async_trait]
    impl DocumentReadAuthorizer for TestAuthorizer {
        async fn authorize_session_read(
            &self,
            _actor: &QueryActorContext,
            session_id: &SessionId,
            _query_type: &QueryType,
        ) -> Result<(), DocumentReadAuthorizationError> {
            self.calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(session_id.clone());
            if self.allow {
                Ok(())
            } else {
                Err(DocumentReadAuthorizationError::Unauthorized)
            }
        }
    }

    struct Fixture {
        document: DocumentDescriptor,
        asset: SourceAssetDescriptor,
        documents: Arc<MemoryDocumentCatalog>,
        read_service: Arc<DocumentReadService>,
    }

    fn fixture() -> Fixture {
        let asset = SourceAssetDescriptor::new(
            AssetId::new(),
            "application/pdf",
            12,
            ContentHash::from_bytes([0x77; 32]),
            Some("lesson.pdf".to_owned()),
        )
        .expect("asset");
        let session_id = SessionId::new();
        let input = DocumentComposeInput::new(
            session_id,
            DocumentType::Pdf,
            asset.clone(),
            Some("Lesson".to_owned()),
            vec![
                DocumentSourcePage::new(
                    0,
                    PageDisplayGeometry::new(100.0, 200.0, PageRotation::Deg0).expect("geometry"),
                ),
                DocumentSourcePage::new(
                    1,
                    PageDisplayGeometry::new(300.0, 400.0, PageRotation::Deg90).expect("geometry"),
                ),
            ],
        )
        .expect("input");
        let composition = DocumentComposer::new().compose(input).expect("composition");
        let document = composition.document().clone();
        let mut canvases = HashMap::new();
        for entry in composition.page_canvases() {
            canvases.insert(entry.canvas().canvas_id().clone(), entry.canvas().clone());
        }
        let documents = Arc::new(MemoryDocumentCatalog::new());
        assert!(matches!(
            documents.insert(document.clone()),
            crate::DocumentInsertOutcome::Inserted
        ));
        let read_service = Arc::new(DocumentReadService::new(
            Arc::clone(&documents) as Arc<dyn DocumentCatalog>,
            Arc::new(TestAssetCatalog {
                asset: Some(asset.clone()),
                calls: Arc::new(AtomicUsize::new(0)),
                unavailable: false,
            }),
            Arc::new(TestCanvasCatalog {
                canvases,
                calls: Arc::new(AtomicUsize::new(0)),
                unavailable: false,
            }),
        ));
        Fixture {
            document,
            asset,
            documents,
            read_service,
        }
    }

    fn request(query_type: &str, value: serde_json::Value) -> QueryRequest {
        let query_type = QueryType::new(query_type).expect("query type");
        let payload = serde_json::from_value(value).expect("object payload");
        QueryRequest::new(orbitrelay_protocol::MessageId::new(), query_type, payload)
    }

    fn executor(fixture: &Fixture, allow: bool) -> RegisteredQueryExecutor {
        let authorizer = Arc::new(TestAuthorizer {
            allow,
            calls: Arc::new(Mutex::new(Vec::new())),
        });
        let list = DocumentListQueryHandler::new(
            Arc::clone(&fixture.documents) as Arc<dyn DocumentCatalog>,
            Arc::clone(&authorizer) as Arc<dyn DocumentReadAuthorizer>,
        )
        .expect("list handler");
        let get = DocumentGetQueryHandler::new(
            Arc::clone(&fixture.documents) as Arc<dyn DocumentCatalog>,
            Arc::clone(&fixture.read_service),
            authorizer,
        )
        .expect("get handler");
        let mut registry = QueryRegistry::new();
        registry.register(Arc::new(list)).expect("register list");
        registry.register(Arc::new(get)).expect("register get");
        RegisteredQueryExecutor::new(Arc::new(registry))
    }

    #[tokio::test]
    async fn document_list_and_get_return_typed_success_payloads() {
        let fixture = fixture();
        let executor = executor(&fixture, true);
        let list = executor
            .execute(
                QueryActorContext::new(ActorId::new()),
                request(
                    "document.list",
                    json!({ "session_id": fixture.document.session_id() }),
                ),
            )
            .await;
        assert!(matches!(list.result(), QueryResult::Success(_)));

        let get = executor
            .execute(
                QueryActorContext::new(ActorId::new()),
                request(
                    "document.get",
                    json!({ "document_id": fixture.document.document_id() }),
                ),
            )
            .await;
        let QueryResult::Success(payload) = get.into_result() else {
            panic!("document.get should succeed")
        };
        let value = serde_json::to_value(payload).expect("payload value");
        let dto: DocumentViewDto = serde_json::from_value(value).expect("typed view DTO");
        assert_eq!(dto.document().document_id(), fixture.document.document_id());
        assert_eq!(dto.page_canvases().len(), 2);
        assert_eq!(dto.source_asset().asset_id(), fixture.asset.asset_id());
    }

    #[tokio::test]
    async fn invalid_payload_unknown_fields_and_missing_document_are_safe_failures() {
        let fixture = fixture();
        let executor = executor(&fixture, true);
        for payload in [
            json!({ "session_id": fixture.document.session_id(), "extra": true }),
            json!({ "session_id": "not-a-uuid" }),
        ] {
            let response = executor
                .execute(
                    QueryActorContext::new(ActorId::new()),
                    request("document.list", payload),
                )
                .await;
            assert!(
                matches!(response.result(), QueryResult::Error(error) if error.code() == QueryFailureCode::InvalidQuery)
            );
        }
        let response = executor
            .execute(
                QueryActorContext::new(ActorId::new()),
                request(
                    "document.get",
                    json!({ "document_id": orbitrelay_document::DocumentId::new() }),
                ),
            )
            .await;
        assert!(
            matches!(response.result(), QueryResult::Error(error) if error.code() == QueryFailureCode::NotFound)
        );
    }

    #[tokio::test]
    async fn list_authorization_and_catalog_failures_map_to_stable_errors() {
        let fixture = fixture();
        let denied = executor(&fixture, false)
            .execute(
                QueryActorContext::new(ActorId::new()),
                request(
                    "document.list",
                    json!({ "session_id": fixture.document.session_id() }),
                ),
            )
            .await;
        assert!(
            matches!(denied.result(), QueryResult::Error(error) if error.code() == QueryFailureCode::Unauthorized)
        );

        struct FailingCatalog;
        #[async_trait]
        impl DocumentCatalog for FailingCatalog {
            async fn get_document(
                &self,
                _document_id: &orbitrelay_document::DocumentId,
            ) -> Result<Option<DocumentDescriptor>, DocumentCatalogError> {
                Err(DocumentCatalogError::new("secret backend detail"))
            }

            async fn list_documents(
                &self,
                _session_id: &SessionId,
            ) -> Result<Vec<DocumentSummary>, DocumentCatalogError> {
                Err(DocumentCatalogError::new("secret backend detail"))
            }
        }
        let handler = DocumentListQueryHandler::new(
            Arc::new(FailingCatalog),
            Arc::new(TestAuthorizer {
                allow: true,
                calls: Arc::new(Mutex::new(Vec::new())),
            }),
        )
        .expect("handler");
        let mut registry = QueryRegistry::new();
        registry.register(Arc::new(handler)).expect("register");
        let response = RegisteredQueryExecutor::new(Arc::new(registry))
            .execute(
                QueryActorContext::new(ActorId::new()),
                request(
                    "document.list",
                    json!({ "session_id": fixture.document.session_id() }),
                ),
            )
            .await;
        assert!(
            matches!(response.result(), QueryResult::Error(error) if error.code() == QueryFailureCode::Unavailable && error.retryable())
        );
        assert!(!serde_json::to_string(&response)
            .expect("response")
            .contains("secret backend detail"));
    }

    #[tokio::test]
    async fn unauthorized_get_stops_before_full_view_reads() {
        let fixture = fixture();
        let asset_calls = Arc::new(AtomicUsize::new(0));
        let canvas_calls = Arc::new(AtomicUsize::new(0));
        let authorizer_calls = Arc::new(Mutex::new(Vec::new()));
        let asset = fixture.asset.clone();
        let mut canvases = HashMap::new();
        for page in fixture.document.pages() {
            let layer = orbitrelay_canvas::LayerId::new();
            let space = orbitrelay_canvas::CanvasSpace::new(
                page.display_geometry().width(),
                page.display_geometry().height(),
            )
            .expect("space");
            canvases.insert(
                page.overlay_canvas_id().clone(),
                CanvasDescriptor::new(
                    page.overlay_canvas_id().clone(),
                    fixture.document.session_id().clone(),
                    space,
                    [layer.clone()],
                    layer,
                )
                .expect("canvas"),
            );
        }
        let read_service = Arc::new(DocumentReadService::new(
            Arc::clone(&fixture.documents) as Arc<dyn DocumentCatalog>,
            Arc::new(TestAssetCatalog {
                asset: Some(asset),
                calls: Arc::clone(&asset_calls),
                unavailable: false,
            }),
            Arc::new(TestCanvasCatalog {
                canvases,
                calls: Arc::clone(&canvas_calls),
                unavailable: false,
            }),
        ));
        let get = DocumentGetQueryHandler::new(
            Arc::clone(&fixture.documents) as Arc<dyn DocumentCatalog>,
            read_service,
            Arc::new(TestAuthorizer {
                allow: false,
                calls: Arc::clone(&authorizer_calls),
            }),
        )
        .expect("get handler");
        let mut registry = QueryRegistry::new();
        registry.register(Arc::new(get)).expect("register");
        let response = RegisteredQueryExecutor::new(Arc::new(registry))
            .execute(
                QueryActorContext::new(ActorId::new()),
                request(
                    "document.get",
                    json!({ "document_id": fixture.document.document_id() }),
                ),
            )
            .await;
        assert!(
            matches!(response.result(), QueryResult::Error(error) if error.code() == QueryFailureCode::Unauthorized)
        );
        assert_eq!(authorizer_calls.lock().expect("calls").len(), 1);
        assert_eq!(asset_calls.load(Ordering::SeqCst), 0);
        assert_eq!(canvas_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn missing_asset_and_canvas_are_safe_internal_failures() {
        let fixture = fixture();
        for (asset, canvases) in [
            (None, HashMap::new()),
            (Some(fixture.asset.clone()), HashMap::new()),
        ] {
            let read_service = Arc::new(DocumentReadService::new(
                Arc::clone(&fixture.documents) as Arc<dyn DocumentCatalog>,
                Arc::new(TestAssetCatalog {
                    asset,
                    calls: Arc::new(AtomicUsize::new(0)),
                    unavailable: false,
                }),
                Arc::new(TestCanvasCatalog {
                    canvases,
                    calls: Arc::new(AtomicUsize::new(0)),
                    unavailable: false,
                }),
            ));
            let handler = DocumentGetQueryHandler::new(
                Arc::clone(&fixture.documents) as Arc<dyn DocumentCatalog>,
                read_service,
                Arc::new(TestAuthorizer {
                    allow: true,
                    calls: Arc::new(Mutex::new(Vec::new())),
                }),
            )
            .expect("handler");
            let mut registry = QueryRegistry::new();
            registry.register(Arc::new(handler)).expect("register");
            let response = RegisteredQueryExecutor::new(Arc::new(registry))
                .execute(
                    QueryActorContext::new(ActorId::new()),
                    request(
                        "document.get",
                        json!({ "document_id": fixture.document.document_id() }),
                    ),
                )
                .await;
            assert!(
                matches!(response.result(), QueryResult::Error(error) if error.code() == QueryFailureCode::Internal)
            );
        }
    }
}
