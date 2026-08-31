//! PDF-neutral composition and read models for OrbitRelay Documents.
//!
//! This crate turns normalized source pages into collaboration descriptors and
//! provides read-only catalog ports. It deliberately does not parse files,
//! perform authorization, publish catalog transactions, or execute Actions.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod catalog;
mod composition;
mod error;
mod query;
mod read_model;

pub use catalog::{DocumentCatalog, DocumentInsertOutcome, MemoryDocumentCatalog};
pub use composition::{
    default_title_for, DocumentComposeInput, DocumentComposer, DocumentComposition,
    DocumentSourcePage, PageCanvasComposition,
};
pub use error::{DocumentCatalogError, DocumentCompositionError, DocumentReadError};
pub use query::{
    register_document_query_handlers, CanvasDto, CanvasSpaceDto, DocumentDto,
    DocumentGetQueryHandler, DocumentListQueryHandler, DocumentListResultDto, DocumentPageDto,
    DocumentReadAuthorizationError, DocumentReadAuthorizer, DocumentSummaryDto, DocumentViewDto,
    PageCanvasDto, SourceAssetDto, DOCUMENT_GET_QUERY_TYPE, DOCUMENT_LIST_QUERY_TYPE,
};
pub use read_model::{DocumentReadService, DocumentSummary, DocumentView, PageCanvasView};
