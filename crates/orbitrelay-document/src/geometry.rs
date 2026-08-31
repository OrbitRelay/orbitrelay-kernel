//! Library-neutral displayed page geometry.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::DocumentError;

/// A normalized quarter-turn applied by the PDF adapter for display.
///
/// The value describes the source-page-to-displayed-page relationship. It is
/// not an instruction for the Canvas overlay to rotate itself again.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PageRotation {
    /// No rotation.
    Deg0,
    /// A clockwise quarter turn.
    Deg90,
    /// A half turn.
    Deg180,
    /// A clockwise three-quarter turn.
    Deg270,
}

impl PageRotation {
    /// Creates a rotation from one of the supported degree values.
    pub fn from_degrees(degrees: u16) -> Result<Self, DocumentError> {
        match degrees {
            0 => Ok(Self::Deg0),
            90 => Ok(Self::Deg90),
            180 => Ok(Self::Deg180),
            270 => Ok(Self::Deg270),
            degrees => Err(DocumentError::InvalidPageRotation { degrees }),
        }
    }

    /// Returns this rotation as its stable degree representation.
    #[must_use]
    pub const fn degrees(self) -> u16 {
        match self {
            Self::Deg0 => 0,
            Self::Deg90 => 90,
            Self::Deg180 => 180,
            Self::Deg270 => 270,
        }
    }
}

impl Serialize for PageRotation {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u16(self.degrees())
    }
}

impl<'de> Deserialize<'de> for PageRotation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let degrees = u16::deserialize(deserializer)?;
        Self::from_degrees(degrees).map_err(serde::de::Error::custom)
    }
}

/// The final visible logical geometry of one PDF page.
///
/// Width and height are measured in PDF logical points after the PDF adapter
/// has selected the visible crop and applied the page rotation. The coordinate
/// system used by the corresponding Canvas is top-left with Y increasing down.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PageDisplayGeometry {
    width: f64,
    height: f64,
    rotation: PageRotation,
}

impl PageDisplayGeometry {
    /// Creates validated normalized displayed page geometry.
    pub fn new(width: f64, height: f64, rotation: PageRotation) -> Result<Self, DocumentError> {
        if !width.is_finite() || width <= 0.0 {
            return Err(DocumentError::InvalidPageGeometry { dimension: "width" });
        }
        if !height.is_finite() || height <= 0.0 {
            return Err(DocumentError::InvalidPageGeometry {
                dimension: "height",
            });
        }

        Ok(Self {
            width,
            height,
            rotation,
        })
    }

    /// Returns the normalized displayed width in PDF logical points.
    #[must_use]
    pub const fn width(&self) -> f64 {
        self.width
    }

    /// Returns the normalized displayed height in PDF logical points.
    #[must_use]
    pub const fn height(&self) -> f64 {
        self.height
    }

    /// Returns the source-page-to-display rotation.
    #[must_use]
    pub const fn rotation(&self) -> PageRotation {
        self.rotation
    }
}

impl<'de> Deserialize<'de> for PageDisplayGeometry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            width: f64,
            height: f64,
            rotation: PageRotation,
        }

        let fields = Fields::deserialize(deserializer)?;
        Self::new(fields.width, fields.height, fields.rotation).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::{PageDisplayGeometry, PageRotation};
    use crate::DocumentError;

    #[test]
    fn accepts_all_supported_rotations_without_swapping_dimensions() {
        for rotation in [
            PageRotation::Deg0,
            PageRotation::Deg90,
            PageRotation::Deg180,
            PageRotation::Deg270,
        ] {
            let geometry = PageDisplayGeometry::new(595.0, 842.0, rotation)
                .expect("normalized geometry should be valid");
            assert_eq!(geometry.width(), 595.0);
            assert_eq!(geometry.height(), 842.0);
        }
    }

    #[test]
    fn rejects_invalid_dimensions_and_rotation() {
        assert!(matches!(
            PageDisplayGeometry::new(0.0, 10.0, PageRotation::Deg0),
            Err(DocumentError::InvalidPageGeometry { dimension: "width" })
        ));
        assert!(matches!(
            PageDisplayGeometry::new(10.0, f64::NAN, PageRotation::Deg0),
            Err(DocumentError::InvalidPageGeometry {
                dimension: "height"
            })
        ));
        assert!(matches!(
            PageRotation::from_degrees(45),
            Err(DocumentError::InvalidPageRotation { degrees: 45 })
        ));
    }

    #[test]
    fn serializes_rotation_as_stable_numeric_degrees() {
        let geometry = PageDisplayGeometry::new(100.0, 200.0, PageRotation::Deg90)
            .expect("geometry should be valid");
        let encoded = serde_json::to_string(&geometry).expect("geometry should serialize");
        assert_eq!(encoded, r#"{"width":100.0,"height":200.0,"rotation":90}"#);
        let decoded: PageDisplayGeometry =
            serde_json::from_str(&encoded).expect("geometry should deserialize");
        assert_eq!(decoded, geometry);
    }
}
