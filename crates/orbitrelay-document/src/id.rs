//! Strong Document and Page identifiers.

use std::{fmt, str::FromStr};

use orbitrelay_core::{CoreError, EntityId};
use serde::{Deserialize, Serialize};

macro_rules! define_document_id {
    ($(#[$attribute:meta])* $name:ident) => {
        $(#[$attribute])*
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(EntityId);

        impl $name {
            /// Creates a new random UUID v4 identifier.
            #[must_use]
            #[allow(
                clippy::new_without_default,
                reason = "creating a domain identity must remain an explicit operation"
            )]
            pub fn new() -> Self {
                Self(EntityId::new())
            }

            /// Wraps an existing core entity identifier.
            #[must_use]
            pub const fn from_entity_id(value: EntityId) -> Self {
                Self(value)
            }

            /// Returns the wrapped core entity identifier.
            #[must_use]
            pub const fn as_entity_id(&self) -> &EntityId {
                &self.0
            }

            /// Parses an identifier from its UUID string representation.
            pub fn parse(value: &str) -> Result<Self, CoreError> {
                Ok(Self(EntityId::parse(value)?))
            }
        }

        impl FromStr for $name {
            type Err = CoreError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::parse(value)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }
    };
}

define_document_id!(
    /// Identifies one Document collaboration object.
    DocumentId
);

define_document_id!(
    /// Identifies one stable page within a Document.
    PageId
);

#[cfg(test)]
mod tests {
    use std::any::TypeId;

    use orbitrelay_asset::AssetId;
    use orbitrelay_canvas::CanvasId;
    use orbitrelay_protocol::SessionId;

    use super::{DocumentId, PageId};

    #[test]
    fn document_and_page_ids_are_distinct_from_other_domain_ids() {
        assert_ne!(TypeId::of::<DocumentId>(), TypeId::of::<PageId>());
        assert_ne!(TypeId::of::<DocumentId>(), TypeId::of::<AssetId>());
        assert_ne!(TypeId::of::<DocumentId>(), TypeId::of::<CanvasId>());
        assert_ne!(TypeId::of::<PageId>(), TypeId::of::<AssetId>());
        assert_ne!(TypeId::of::<PageId>(), TypeId::of::<SessionId>());
    }

    #[test]
    fn ids_round_trip_through_json_and_strings() {
        let document_id = DocumentId::new();
        let page_id = PageId::new();

        let encoded = serde_json::to_string(&document_id).expect("DocumentId should serialize");
        let decoded: DocumentId =
            serde_json::from_str(&encoded).expect("DocumentId should deserialize");
        assert_eq!(decoded, document_id);
        assert_eq!(
            document_id.to_string().parse::<DocumentId>().unwrap(),
            document_id
        );

        let encoded_page = serde_json::to_string(&page_id).expect("PageId should serialize");
        let decoded_page: PageId =
            serde_json::from_str(&encoded_page).expect("PageId should deserialize");
        assert_eq!(decoded_page, page_id);
    }
}
