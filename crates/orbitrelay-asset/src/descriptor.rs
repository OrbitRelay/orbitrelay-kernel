//! Immutable source Asset metadata.

use serde::{Deserialize, Serialize};

use crate::{AssetError, AssetId, ContentHash};

/// Metadata describing one immutable source Asset.
///
/// The descriptor contains no filesystem path, object-storage key, URL, or
/// other backend location. `byte_length` may be zero; a concrete consumer such
/// as the PDF adapter decides whether empty bytes are meaningful.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceAssetDescriptor {
    asset_id: AssetId,
    media_type: String,
    byte_length: u64,
    content_hash: ContentHash,
    original_filename: Option<String>,
}

impl SourceAssetDescriptor {
    /// Creates validated metadata for an immutable source Asset.
    pub fn new(
        asset_id: AssetId,
        media_type: impl Into<String>,
        byte_length: u64,
        content_hash: ContentHash,
        original_filename: Option<String>,
    ) -> Result<Self, AssetError> {
        let media_type = media_type.into();
        if media_type.trim().is_empty() {
            return Err(AssetError::InvalidMediaType);
        }
        if original_filename
            .as_deref()
            .is_some_and(|filename| filename.trim().is_empty())
        {
            return Err(AssetError::InvalidOriginalFilename);
        }

        Ok(Self {
            asset_id,
            media_type,
            byte_length,
            content_hash,
            original_filename,
        })
    }

    /// Returns the opaque Asset identity.
    #[must_use]
    pub const fn asset_id(&self) -> &AssetId {
        &self.asset_id
    }

    /// Returns the supplied media type metadata.
    #[must_use]
    pub fn media_type(&self) -> &str {
        &self.media_type
    }

    /// Returns the byte length metadata.
    #[must_use]
    pub const fn byte_length(&self) -> u64 {
        self.byte_length
    }

    /// Returns the validated SHA-256 content hash.
    #[must_use]
    pub const fn content_hash(&self) -> &ContentHash {
        &self.content_hash
    }

    /// Returns the optional display filename metadata.
    #[must_use]
    pub fn original_filename(&self) -> Option<&str> {
        self.original_filename.as_deref()
    }
}

impl<'de> Deserialize<'de> for SourceAssetDescriptor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            asset_id: AssetId,
            media_type: String,
            byte_length: u64,
            content_hash: ContentHash,
            original_filename: Option<String>,
        }

        let fields = Fields::deserialize(deserializer)?;
        Self::new(
            fields.asset_id,
            fields.media_type,
            fields.byte_length,
            fields.content_hash,
            fields.original_filename,
        )
        .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::SourceAssetDescriptor;
    use crate::{AssetError, AssetId, ContentHash};

    fn hash() -> ContentHash {
        ContentHash::from_bytes([0x11; 32])
    }

    #[test]
    fn accepts_zero_length_asset_metadata() {
        let descriptor = SourceAssetDescriptor::new(
            AssetId::new(),
            "application/pdf",
            0,
            hash(),
            Some("lesson.pdf".to_owned()),
        )
        .expect("zero length is valid generic metadata");

        assert_eq!(descriptor.byte_length(), 0);
        assert_eq!(descriptor.media_type(), "application/pdf");
        assert_eq!(descriptor.original_filename(), Some("lesson.pdf"));
    }

    #[test]
    fn rejects_blank_media_type_and_filename() {
        assert!(matches!(
            SourceAssetDescriptor::new(AssetId::new(), "  ", 1, hash(), None),
            Err(AssetError::InvalidMediaType)
        ));
        assert!(matches!(
            SourceAssetDescriptor::new(
                AssetId::new(),
                "application/pdf",
                1,
                hash(),
                Some("   ".to_owned())
            ),
            Err(AssetError::InvalidOriginalFilename)
        ));
    }

    #[test]
    fn descriptor_round_trips_through_json_with_validation() {
        let descriptor =
            SourceAssetDescriptor::new(AssetId::new(), "application/pdf", 42, hash(), None)
                .expect("descriptor should be valid");
        let encoded = serde_json::to_string(&descriptor).expect("descriptor should serialize");
        let decoded: SourceAssetDescriptor =
            serde_json::from_str(&encoded).expect("descriptor should deserialize");

        assert_eq!(decoded, descriptor);
    }

    #[test]
    fn malformed_hash_is_rejected_at_json_boundary() {
        let value = serde_json::json!({
            "asset_id": AssetId::new(),
            "media_type": "application/pdf",
            "byte_length": 1,
            "content_hash": "not-a-hash",
            "original_filename": null
        });

        assert!(serde_json::from_value::<SourceAssetDescriptor>(value).is_err());
    }
}
