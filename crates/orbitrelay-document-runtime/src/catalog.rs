//! Document metadata catalog port and immutable memory adapter.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use orbitrelay_document::{DocumentDescriptor, DocumentId};
use orbitrelay_protocol::SessionId;

use crate::{DocumentCatalogError, DocumentSummary};

/// Read-only access to immutable Document collaboration descriptors.
#[async_trait]
pub trait DocumentCatalog: Send + Sync {
    /// Returns a Document descriptor, or `Ok(None)` when it does not exist.
    async fn get_document(
        &self,
        document_id: &DocumentId,
    ) -> Result<Option<DocumentDescriptor>, DocumentCatalogError>;

    /// Lists lightweight summaries in stable insertion order for one Session.
    async fn list_documents(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<DocumentSummary>, DocumentCatalogError>;
}

/// Outcome of an immutable memory-catalog setup insertion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentInsertOutcome {
    /// The Document identity was newly registered.
    Inserted,
    /// The exact same descriptor was already registered.
    Existing,
    /// The identity was registered with different immutable content.
    Conflict,
}

#[derive(Default)]
struct CatalogState {
    documents: HashMap<DocumentId, DocumentDescriptor>,
    session_order: HashMap<SessionId, Vec<DocumentId>>,
}

/// Cloneable, shared in-memory read catalog for development and tests.
///
/// [`MemoryDocumentCatalog::insert`] is explicitly a setup/registration API;
/// it is not a future Document mutation or upload protocol.
#[derive(Clone, Default)]
pub struct MemoryDocumentCatalog {
    state: Arc<RwLock<CatalogState>>,
}

impl MemoryDocumentCatalog {
    /// Creates an empty shared memory catalog.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers an immutable descriptor without overwriting existing data.
    ///
    /// Equal repeated registration is idempotent and never duplicates the
    /// Session's ordering list. A conflicting descriptor is left untouched.
    pub fn insert(&self, document: DocumentDescriptor) -> DocumentInsertOutcome {
        let document_id = document.document_id().clone();
        let session_id = document.session_id().clone();
        let mut state = self
            .state
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if let Some(existing) = state.documents.get(&document_id) {
            return if existing == &document {
                DocumentInsertOutcome::Existing
            } else {
                DocumentInsertOutcome::Conflict
            };
        }

        state.documents.insert(document_id.clone(), document);
        state
            .session_order
            .entry(session_id)
            .or_default()
            .push(document_id);
        DocumentInsertOutcome::Inserted
    }
}

#[async_trait]
impl DocumentCatalog for MemoryDocumentCatalog {
    async fn get_document(
        &self,
        document_id: &DocumentId,
    ) -> Result<Option<DocumentDescriptor>, DocumentCatalogError> {
        let state = self
            .state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(state.documents.get(document_id).cloned())
    }

    async fn list_documents(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<DocumentSummary>, DocumentCatalogError> {
        let state = self
            .state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(document_ids) = state.session_order.get(session_id) else {
            return Ok(Vec::new());
        };

        document_ids
            .iter()
            .map(|document_id| {
                let document = state.documents.get(document_id).ok_or_else(|| {
                    DocumentCatalogError::new("session ordering references a missing Document")
                })?;
                DocumentSummary::from_document(document)
                    .map_err(|_| DocumentCatalogError::new("Document page count overflowed"))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use orbitrelay_asset::{AssetId, ContentHash, SourceAssetDescriptor};
    use orbitrelay_document::{
        DocumentDescriptor, DocumentType, PageDisplayGeometry, PageRotation,
    };
    use orbitrelay_protocol::SessionId;

    use super::{DocumentCatalog, DocumentInsertOutcome, MemoryDocumentCatalog};
    use crate::{DocumentComposeInput, DocumentComposer, DocumentSourcePage};

    fn composition(session_id: SessionId, title: &str) -> DocumentDescriptor {
        let asset = SourceAssetDescriptor::new(
            AssetId::new(),
            "application/pdf",
            1,
            ContentHash::from_bytes([0x22; 32]),
            Some("file.pdf".into()),
        )
        .expect("asset");
        let input = DocumentComposeInput::new(
            session_id,
            DocumentType::Pdf,
            asset,
            Some(title.into()),
            vec![DocumentSourcePage::new(
                0,
                PageDisplayGeometry::new(10.0, 20.0, PageRotation::Deg0).expect("geometry"),
            )],
        )
        .expect("input");
        DocumentComposer::new()
            .compose(input)
            .expect("composition")
            .document()
            .clone()
    }

    #[tokio::test]
    async fn inserts_idempotently_and_lists_by_session_order() {
        let catalog = MemoryDocumentCatalog::new();
        let session = SessionId::new();
        let first = composition(session.clone(), "first");
        let second = composition(session.clone(), "second");
        assert_eq!(
            catalog.insert(first.clone()),
            DocumentInsertOutcome::Inserted
        );
        assert_eq!(
            catalog.insert(first.clone()),
            DocumentInsertOutcome::Existing
        );
        assert_eq!(
            catalog.insert(second.clone()),
            DocumentInsertOutcome::Inserted
        );

        let listed = catalog.list_documents(&session).await.expect("list");
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].document_id(), first.document_id());
        assert_eq!(listed[0].title(), "first");
        assert_eq!(listed[0].document_type(), DocumentType::Pdf);
        assert_eq!(listed[0].page_count(), 1);
        assert_eq!(listed[0].source_asset_id(), first.source_asset_id());
        assert_eq!(listed[1].document_id(), second.document_id());
        assert!(catalog
            .list_documents(&SessionId::new())
            .await
            .expect("other session list")
            .is_empty());
    }

    #[tokio::test]
    async fn conflicts_do_not_overwrite_or_cross_session_move() {
        let catalog = MemoryDocumentCatalog::new();
        let first = composition(SessionId::new(), "first");
        let mut conflicting = composition(first.session_id().clone(), "other");
        conflicting = DocumentDescriptor::new(
            first.document_id().clone(),
            conflicting.session_id().clone(),
            conflicting.document_type(),
            conflicting.source_asset_id().clone(),
            conflicting.title().to_owned(),
            conflicting.pages().to_vec(),
        )
        .expect("conflicting descriptor");
        assert_eq!(
            catalog.insert(first.clone()),
            DocumentInsertOutcome::Inserted
        );
        assert_eq!(catalog.insert(conflicting), DocumentInsertOutcome::Conflict);
        assert_eq!(
            catalog
                .get_document(first.document_id())
                .await
                .expect("get")
                .expect("present"),
            first
        );

        let moved = DocumentDescriptor::new(
            first.document_id().clone(),
            SessionId::new(),
            first.document_type(),
            first.source_asset_id().clone(),
            first.title().to_owned(),
            first.pages().to_vec(),
        )
        .expect("cross-session descriptor");
        assert_eq!(catalog.insert(moved), DocumentInsertOutcome::Conflict);
    }
}
