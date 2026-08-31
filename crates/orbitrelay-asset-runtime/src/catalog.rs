//! Metadata lookup port for immutable Assets.

use async_trait::async_trait;
use orbitrelay_asset::{AssetId, SourceAssetDescriptor};

use crate::AssetCatalogError;

/// Reads immutable Asset metadata without reading the Asset bytes.
#[async_trait]
pub trait AssetCatalog: Send + Sync {
    /// Returns metadata for an Asset, or `Ok(None)` when it does not exist.
    async fn get_asset(
        &self,
        asset_id: &AssetId,
    ) -> Result<Option<SourceAssetDescriptor>, AssetCatalogError>;
}
