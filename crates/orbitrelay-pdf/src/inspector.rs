//! Asset-port backed PDF inspection facade.

use std::sync::Arc;

use orbitrelay_asset::AssetId;
use orbitrelay_asset_runtime::{read_asset_fully, AssetCatalog, AssetReadAllError, AssetReader};

use crate::{parser, PdfDocumentMetadata, PdfError, PdfInspectionLimits};

/// Inspects PDF metadata through immutable Asset access ports.
#[derive(Clone)]
pub struct PdfInspector {
    catalog: Arc<dyn AssetCatalog>,
    reader: Arc<dyn AssetReader>,
    limits: PdfInspectionLimits,
}

impl PdfInspector {
    /// Creates an inspector backed by a metadata catalog and range reader.
    #[must_use]
    pub fn new(
        catalog: Arc<dyn AssetCatalog>,
        reader: Arc<dyn AssetReader>,
        limits: PdfInspectionLimits,
    ) -> Self {
        Self {
            catalog,
            reader,
            limits,
        }
    }

    /// Returns the configured inspection limits.
    #[must_use]
    pub const fn limits(&self) -> PdfInspectionLimits {
        self.limits
    }

    /// Reads and parses one immutable PDF Asset.
    ///
    /// No collaboration identity is created here. The returned Asset identity
    /// and page indexes are later combined with Session/Document/Page/Canvas
    /// identities by a composition layer.
    pub async fn inspect(&self, asset_id: &AssetId) -> Result<PdfDocumentMetadata, PdfError> {
        let descriptor = self
            .catalog
            .get_asset(asset_id)
            .await
            .map_err(|_| PdfError::AssetUnavailable)?
            .ok_or_else(|| PdfError::AssetNotFound {
                asset_id: asset_id.clone(),
            })?;

        if descriptor.byte_length() > self.limits.max_asset_bytes() {
            return Err(PdfError::AssetTooLarge {
                length: descriptor.byte_length(),
                max_bytes: self.limits.max_asset_bytes(),
            });
        }

        let bytes = read_asset_fully(
            self.catalog.as_ref(),
            self.reader.as_ref(),
            asset_id,
            self.limits.max_asset_bytes(),
        )
        .await
        .map_err(|error| map_read_error(error, asset_id))?;

        parser::inspect_bytes(asset_id.clone(), &bytes, self.limits)
    }
}

fn map_read_error(error: AssetReadAllError, asset_id: &AssetId) -> PdfError {
    match error {
        AssetReadAllError::NotFound { .. } => PdfError::AssetNotFound {
            asset_id: asset_id.clone(),
        },
        AssetReadAllError::AssetTooLarge {
            length, max_bytes, ..
        } => PdfError::AssetTooLarge { length, max_bytes },
        AssetReadAllError::CatalogUnavailable { .. } => PdfError::AssetUnavailable,
        AssetReadAllError::Reader(_)
        | AssetReadAllError::LengthOverflow { .. }
        | AssetReadAllError::InvalidChunk { .. } => PdfError::ReadFailed,
        _ => PdfError::ReadFailed,
    }
}
