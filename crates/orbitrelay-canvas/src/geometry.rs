//! Logical Canvas coordinate values and bounds.

use serde::{de::Error as _, Deserialize, Deserializer, Serialize};

use crate::CanvasError;

/// A finite point in Canvas logical coordinates.
///
/// Point construction validates finiteness only. Use [`CanvasSpace::validate_point`]
/// when a trusted Canvas space is available.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct CanvasPoint {
    x: f64,
    y: f64,
}

impl CanvasPoint {
    /// Creates a finite logical coordinate without applying Canvas bounds.
    pub fn new(x: f64, y: f64) -> Result<Self, CanvasError> {
        if !x.is_finite() {
            return Err(CanvasError::InvalidCoordinate { coordinate: "x" });
        }
        if !y.is_finite() {
            return Err(CanvasError::InvalidCoordinate { coordinate: "y" });
        }
        Ok(Self { x, y })
    }

    /// Returns the horizontal logical coordinate.
    #[must_use]
    pub const fn x(&self) -> f64 {
        self.x
    }

    /// Returns the vertical logical coordinate.
    #[must_use]
    pub const fn y(&self) -> f64 {
        self.y
    }
}

impl<'de> Deserialize<'de> for CanvasPoint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct PointFields {
            x: f64,
            y: f64,
        }

        let fields = PointFields::deserialize(deserializer)?;
        Self::new(fields.x, fields.y).map_err(D::Error::custom)
    }
}

/// Immutable finite bounds for one logical Canvas coordinate system.
///
/// The origin is at the top left. X increases to the right and Y increases
/// downward. Both boundary edges are inclusive.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct CanvasSpace {
    width: f64,
    height: f64,
}

impl CanvasSpace {
    /// Creates a Canvas space with finite, positive dimensions.
    pub fn new(width: f64, height: f64) -> Result<Self, CanvasError> {
        if !width.is_finite() || width <= 0.0 {
            return Err(CanvasError::InvalidCanvasSpace { dimension: "width" });
        }
        if !height.is_finite() || height <= 0.0 {
            return Err(CanvasError::InvalidCanvasSpace {
                dimension: "height",
            });
        }
        Ok(Self { width, height })
    }

    /// Returns the logical Canvas width.
    #[must_use]
    pub const fn width(&self) -> f64 {
        self.width
    }

    /// Returns the logical Canvas height.
    #[must_use]
    pub const fn height(&self) -> f64 {
        self.height
    }

    /// Reports whether a finite point lies within the inclusive bounds.
    #[must_use]
    pub fn contains(&self, point: &CanvasPoint) -> bool {
        point.x >= 0.0 && point.x <= self.width && point.y >= 0.0 && point.y <= self.height
    }

    /// Validates a point against these inclusive Canvas bounds.
    pub fn validate_point(&self, point: &CanvasPoint) -> Result<(), CanvasError> {
        if point.x < 0.0 || point.x > self.width {
            return Err(CanvasError::InvalidCoordinate { coordinate: "x" });
        }
        if point.y < 0.0 || point.y > self.height {
            return Err(CanvasError::InvalidCoordinate { coordinate: "y" });
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for CanvasSpace {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct SpaceFields {
            width: f64,
            height: f64,
        }

        let fields = SpaceFields::deserialize(deserializer)?;
        Self::new(fields.width, fields.height).map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::{CanvasPoint, CanvasSpace};

    #[test]
    fn accepts_valid_canvas_space() {
        let space = CanvasSpace::new(1920.0, 1080.0).expect("space should be valid");

        assert_eq!(space.width(), 1920.0);
        assert_eq!(space.height(), 1080.0);
    }

    #[test]
    fn rejects_invalid_canvas_dimensions() {
        assert!(CanvasSpace::new(0.0, 10.0).is_err());
        assert!(CanvasSpace::new(10.0, -1.0).is_err());
        assert!(CanvasSpace::new(f64::NAN, 10.0).is_err());
        assert!(CanvasSpace::new(10.0, f64::INFINITY).is_err());
        assert!(CanvasSpace::new(f64::NEG_INFINITY, 10.0).is_err());
    }

    #[test]
    fn point_requires_only_finite_coordinates() {
        let outside = CanvasPoint::new(-10.0, 5000.0).expect("finite point should be valid");

        assert_eq!(outside.x(), -10.0);
        assert_eq!(outside.y(), 5000.0);
        assert!(CanvasPoint::new(f64::NAN, 0.0).is_err());
        assert!(CanvasPoint::new(0.0, f64::INFINITY).is_err());
        assert!(CanvasPoint::new(f64::NEG_INFINITY, 0.0).is_err());
    }

    #[test]
    fn canvas_space_validates_inclusive_point_bounds() {
        let space = CanvasSpace::new(100.0, 50.0).expect("space should be valid");
        let origin = CanvasPoint::new(0.0, 0.0).expect("point should be valid");
        let far_edge = CanvasPoint::new(100.0, 50.0).expect("point should be valid");
        let outside = CanvasPoint::new(100.1, 25.0).expect("point should be finite");

        assert!(space.contains(&origin));
        assert!(space.contains(&far_edge));
        assert!(!space.contains(&outside));
        space
            .validate_point(&far_edge)
            .expect("inclusive edge should be valid");
        assert!(space.validate_point(&outside).is_err());
    }
}
