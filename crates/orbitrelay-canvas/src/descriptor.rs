//! Trusted Canvas metadata used by domain-aware runtime adapters.

use std::collections::BTreeSet;

use orbitrelay_protocol::SessionId;

use crate::{CanvasError, CanvasId, CanvasSpace, LayerId};

/// Immutable domain description of one Canvas and its valid layers.
///
/// This value is neither a persistence record nor a transport message. A
/// runtime catalog is responsible for supplying trusted descriptors.
#[derive(Clone, Debug, PartialEq)]
pub struct CanvasDescriptor {
    canvas_id: CanvasId,
    session_id: SessionId,
    space: CanvasSpace,
    layer_ids: BTreeSet<LayerId>,
    default_layer_id: LayerId,
}

impl CanvasDescriptor {
    /// Creates a descriptor with at least one layer and a valid default layer.
    pub fn new(
        canvas_id: CanvasId,
        session_id: SessionId,
        space: CanvasSpace,
        layer_ids: impl IntoIterator<Item = LayerId>,
        default_layer_id: LayerId,
    ) -> Result<Self, CanvasError> {
        let layer_ids = layer_ids.into_iter().collect::<BTreeSet<_>>();
        if layer_ids.is_empty() {
            return Err(CanvasError::EmptyLayerSet);
        }
        if !layer_ids.contains(&default_layer_id) {
            return Err(CanvasError::DefaultLayerNotFound { default_layer_id });
        }

        Ok(Self {
            canvas_id,
            session_id,
            space,
            layer_ids,
            default_layer_id,
        })
    }

    /// Returns the described Canvas identifier.
    #[must_use]
    pub const fn canvas_id(&self) -> &CanvasId {
        &self.canvas_id
    }

    /// Returns the collaboration session that owns this Canvas.
    #[must_use]
    pub const fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    /// Returns the trusted logical coordinate space.
    #[must_use]
    pub const fn space(&self) -> &CanvasSpace {
        &self.space
    }

    /// Returns all layers currently valid for this Canvas.
    #[must_use]
    pub const fn layer_ids(&self) -> &BTreeSet<LayerId> {
        &self.layer_ids
    }

    /// Returns the default layer for new Strokes.
    #[must_use]
    pub const fn default_layer_id(&self) -> &LayerId {
        &self.default_layer_id
    }

    /// Reports whether a layer belongs to this Canvas descriptor.
    #[must_use]
    pub fn contains_layer(&self, layer_id: &LayerId) -> bool {
        self.layer_ids.contains(layer_id)
    }
}

#[cfg(test)]
mod tests {
    use std::any::TypeId;

    use orbitrelay_protocol::SessionId;

    use super::CanvasDescriptor;
    use crate::{CanvasError, CanvasId, CanvasSpace, LayerId};

    #[test]
    fn creates_descriptor_and_reports_layer_membership() {
        let canvas_id = CanvasId::new();
        let session_id = SessionId::new();
        let default_layer = LayerId::new();
        let secondary_layer = LayerId::new();
        let space = CanvasSpace::new(100.0, 50.0).expect("space should be valid");
        let descriptor = CanvasDescriptor::new(
            canvas_id.clone(),
            session_id.clone(),
            space,
            [default_layer.clone(), secondary_layer.clone()],
            default_layer.clone(),
        )
        .expect("descriptor should be valid");

        assert_eq!(descriptor.canvas_id(), &canvas_id);
        assert_eq!(descriptor.session_id(), &session_id);
        assert_eq!(descriptor.space(), &space);
        assert_eq!(descriptor.default_layer_id(), &default_layer);
        assert!(descriptor.contains_layer(&secondary_layer));
        assert_eq!(descriptor.layer_ids().len(), 2);
    }

    #[test]
    fn rejects_empty_layers_and_missing_default_layer() {
        let space = CanvasSpace::new(100.0, 50.0).expect("space should be valid");
        assert!(matches!(
            CanvasDescriptor::new(CanvasId::new(), SessionId::new(), space, [], LayerId::new(),),
            Err(CanvasError::EmptyLayerSet)
        ));

        let only_layer = LayerId::new();
        assert!(matches!(
            CanvasDescriptor::new(
                CanvasId::new(),
                SessionId::new(),
                space,
                [only_layer],
                LayerId::new(),
            ),
            Err(CanvasError::DefaultLayerNotFound { .. })
        ));
    }

    #[test]
    fn session_and_canvas_identifiers_remain_distinct_types() {
        assert_ne!(TypeId::of::<SessionId>(), TypeId::of::<CanvasId>());
    }
}
