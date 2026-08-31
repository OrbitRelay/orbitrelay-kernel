//! Stable Canvas action type names.

/// Begins a Stroke and supplies its initial point chunk.
pub const STROKE_BEGIN_ACTION_TYPE: &str = "canvas.stroke.begin";
/// Appends a point chunk to an active Stroke.
pub const STROKE_APPEND_ACTION_TYPE: &str = "canvas.stroke.append";
/// Completes an active Stroke.
pub const STROKE_END_ACTION_TYPE: &str = "canvas.stroke.end";
/// Cancels an active Stroke.
pub const STROKE_CANCEL_ACTION_TYPE: &str = "canvas.stroke.cancel";
/// Removes a completed Stroke.
pub const STROKE_REMOVE_ACTION_TYPE: &str = "canvas.stroke.remove";

#[cfg(test)]
mod tests {
    use super::{
        STROKE_APPEND_ACTION_TYPE, STROKE_BEGIN_ACTION_TYPE, STROKE_CANCEL_ACTION_TYPE,
        STROKE_END_ACTION_TYPE, STROKE_REMOVE_ACTION_TYPE,
    };

    #[test]
    fn action_type_names_are_stable() {
        assert_eq!(STROKE_BEGIN_ACTION_TYPE, "canvas.stroke.begin");
        assert_eq!(STROKE_APPEND_ACTION_TYPE, "canvas.stroke.append");
        assert_eq!(STROKE_END_ACTION_TYPE, "canvas.stroke.end");
        assert_eq!(STROKE_CANCEL_ACTION_TYPE, "canvas.stroke.cancel");
        assert_eq!(STROKE_REMOVE_ACTION_TYPE, "canvas.stroke.remove");
    }
}
