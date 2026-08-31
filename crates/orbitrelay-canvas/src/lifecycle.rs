//! Pure Stroke lifecycle transition rules.

use serde::{Deserialize, Serialize};

use crate::CanvasError;

/// Persisted lifecycle of one Canvas Stroke.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum StrokeLifecycle {
    /// The Stroke accepts point chunks and a terminal action.
    Active,
    /// The Stroke was completed and may be removed.
    Completed,
    /// The active Stroke was cancelled.
    Cancelled,
    /// The completed Stroke was removed from the visible projection.
    Removed,
}

impl StrokeLifecycle {
    /// Reports whether this state can transition to the supplied state.
    #[must_use]
    pub const fn can_transition_to(self, target: Self) -> bool {
        matches!(
            (self, target),
            (Self::Active, Self::Completed)
                | (Self::Active, Self::Cancelled)
                | (Self::Completed, Self::Removed)
        )
    }

    /// Applies a legal pure lifecycle transition.
    pub fn transition_to(self, target: Self) -> Result<Self, CanvasError> {
        if self.can_transition_to(target) {
            Ok(target)
        } else {
            Err(CanvasError::InvalidStrokeState {
                from: self,
                to: target,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::StrokeLifecycle;

    #[test]
    fn accepts_legal_lifecycle_transitions() {
        assert_eq!(
            StrokeLifecycle::Active
                .transition_to(StrokeLifecycle::Completed)
                .expect("active Stroke should complete"),
            StrokeLifecycle::Completed
        );
        assert!(StrokeLifecycle::Active
            .transition_to(StrokeLifecycle::Cancelled)
            .is_ok());
        assert!(StrokeLifecycle::Completed
            .transition_to(StrokeLifecycle::Removed)
            .is_ok());
    }

    #[test]
    fn rejects_illegal_lifecycle_transitions() {
        assert!(StrokeLifecycle::Active
            .transition_to(StrokeLifecycle::Removed)
            .is_err());
        assert!(StrokeLifecycle::Completed
            .transition_to(StrokeLifecycle::Cancelled)
            .is_err());
        assert!(StrokeLifecycle::Cancelled
            .transition_to(StrokeLifecycle::Removed)
            .is_err());
        assert!(StrokeLifecycle::Removed
            .transition_to(StrokeLifecycle::Active)
            .is_err());
    }
}
