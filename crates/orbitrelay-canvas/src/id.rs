//! Strong identifiers for Canvas domain entities.

use std::{fmt, str::FromStr};

use orbitrelay_core::{CoreError, EntityId};
use serde::{Deserialize, Serialize};

macro_rules! define_canvas_id {
    ($(#[$attribute:meta])* $name:ident) => {
        $(#[$attribute])*
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(EntityId);

        impl $name {
            /// Creates a new random identifier.
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

define_canvas_id!(
    /// Identifies one Canvas within a collaboration session.
    ///
    /// Canvas identifiers are not session or stroke identifiers:
    ///
    /// ```compile_fail
    /// use orbitrelay_canvas::{CanvasId, StrokeId};
    ///
    /// fn accept_canvas(_: CanvasId) {}
    /// accept_canvas(StrokeId::new());
    /// ```
    CanvasId
);

define_canvas_id!(
    /// Identifies one rendering layer within a Canvas.
    LayerId
);

define_canvas_id!(
    /// Identifies one stable Stroke across its begin, append, and terminal actions.
    StrokeId
);

#[cfg(test)]
mod tests {
    use std::any::TypeId;

    use super::{CanvasId, LayerId, StrokeId};

    #[test]
    fn canvas_ids_are_distinct_types() {
        assert_ne!(TypeId::of::<CanvasId>(), TypeId::of::<LayerId>());
        assert_ne!(TypeId::of::<CanvasId>(), TypeId::of::<StrokeId>());
        assert_ne!(TypeId::of::<LayerId>(), TypeId::of::<StrokeId>());
    }

    #[test]
    fn ids_round_trip_through_json_and_strings() {
        let canvas_id = CanvasId::new();
        let layer_id = LayerId::new();
        let stroke_id = StrokeId::new();
        let encoded = serde_json::to_string(&canvas_id).expect("CanvasId should serialize");
        let decoded: CanvasId =
            serde_json::from_str(&encoded).expect("CanvasId should deserialize");
        let encoded_layer = serde_json::to_string(&layer_id).expect("LayerId should serialize");
        let decoded_layer: LayerId =
            serde_json::from_str(&encoded_layer).expect("LayerId should deserialize");
        let encoded_stroke = serde_json::to_string(&stroke_id).expect("StrokeId should serialize");
        let decoded_stroke: StrokeId =
            serde_json::from_str(&encoded_stroke).expect("StrokeId should deserialize");

        assert_eq!(decoded, canvas_id);
        assert_eq!(decoded_layer, layer_id);
        assert_eq!(decoded_stroke, stroke_id);
        assert_eq!(
            canvas_id
                .to_string()
                .parse::<CanvasId>()
                .expect("CanvasId should parse"),
            canvas_id
        );
    }
}
