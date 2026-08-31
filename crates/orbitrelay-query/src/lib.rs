//! Transport-neutral Query request/response models and execution ports.
//!
//! Queries are read operations. They do not produce Actions, Events, or
//! EventPipeline work. Domain crates register typed handlers without making
//! this crate depend on those domains.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod executor;
mod handler;
mod model;
mod registry;

pub use executor::{QueryExecutor, RegisteredQueryExecutor};
pub use handler::{QueryHandler, QueryHandlerError};
pub use model::{
    QueryActorContext, QueryFailure, QueryFailureCode, QueryRequest, QueryResponse, QueryResult,
    QueryType, QueryTypeError,
};
pub use registry::{QueryRegistry, QueryRegistryError};
