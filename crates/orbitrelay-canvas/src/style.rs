//! Canvas Stroke tool and visual style values.

use serde::{de::Error as _, Deserialize, Deserializer, Serialize};

use crate::CanvasError;

/// Explicit red, green, blue, and alpha color channels.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RgbaColor {
    red: u8,
    green: u8,
    blue: u8,
    alpha: u8,
}

impl RgbaColor {
    /// Creates an RGBA color from four independent channels.
    #[must_use]
    pub const fn new(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }

    /// Returns the red channel.
    #[must_use]
    pub const fn red(&self) -> u8 {
        self.red
    }

    /// Returns the green channel.
    #[must_use]
    pub const fn green(&self) -> u8 {
        self.green
    }

    /// Returns the blue channel.
    #[must_use]
    pub const fn blue(&self) -> u8 {
        self.blue
    }

    /// Returns the alpha channel.
    #[must_use]
    pub const fn alpha(&self) -> u8 {
        self.alpha
    }
}

/// Tool used to produce a Stroke.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum StrokeTool {
    /// A persistent freehand pen Stroke.
    Pen,
}

/// Visual style expressed in Canvas logical units and explicit RGBA color.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StrokeStyle {
    width: f64,
    color: RgbaColor,
}

impl StrokeStyle {
    /// Creates a Stroke style with a finite, positive logical width.
    pub fn new(width: f64, color: RgbaColor) -> Result<Self, CanvasError> {
        if !width.is_finite() || width <= 0.0 {
            return Err(CanvasError::InvalidStyle { field: "width" });
        }
        Ok(Self { width, color })
    }

    /// Returns the Stroke width in Canvas logical units.
    #[must_use]
    pub const fn width(&self) -> f64 {
        self.width
    }

    /// Returns the explicit Stroke color.
    #[must_use]
    pub const fn color(&self) -> &RgbaColor {
        &self.color
    }
}

impl<'de> Deserialize<'de> for StrokeStyle {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct StyleFields {
            width: f64,
            color: RgbaColor,
        }

        let fields = StyleFields::deserialize(deserializer)?;
        Self::new(fields.width, fields.color).map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::{RgbaColor, StrokeStyle, StrokeTool};

    #[test]
    fn rgba_color_round_trips_with_explicit_fields() {
        let color = RgbaColor::new(12, 34, 56, 78);
        let encoded = serde_json::to_string(&color).expect("color should serialize");
        let decoded: RgbaColor = serde_json::from_str(&encoded).expect("color should deserialize");

        assert_eq!(decoded, color);
        assert_eq!(decoded.red(), 12);
        assert_eq!(decoded.green(), 34);
        assert_eq!(decoded.blue(), 56);
        assert_eq!(decoded.alpha(), 78);
        assert_eq!(encoded, r#"{"red":12,"green":34,"blue":56,"alpha":78}"#);
    }

    #[test]
    fn validates_stroke_width() {
        let color = RgbaColor::new(0, 0, 0, 255);
        let style = StrokeStyle::new(2.5, color).expect("style should be valid");

        assert_eq!(style.width(), 2.5);
        assert_eq!(style.color(), &color);
        assert!(StrokeStyle::new(0.0, color).is_err());
        assert!(StrokeStyle::new(-1.0, color).is_err());
        assert!(StrokeStyle::new(f64::NAN, color).is_err());
        assert!(StrokeStyle::new(f64::INFINITY, color).is_err());
    }

    #[test]
    fn pen_has_stable_json_representation() {
        let encoded = serde_json::to_string(&StrokeTool::Pen).expect("tool should serialize");
        let decoded: StrokeTool = serde_json::from_str(&encoded).expect("tool should deserialize");

        assert_eq!(encoded, r#""pen""#);
        assert_eq!(decoded, StrokeTool::Pen);
    }
}
