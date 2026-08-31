//! Validated cross-backend Asset byte ranges.

use crate::AssetRangeError;

/// A non-empty immutable byte range expressed with backend-independent `u64`
/// offsets.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AssetByteRange {
    offset: u64,
    length: u64,
}

impl AssetByteRange {
    /// Creates `[offset, offset + length)`, rejecting empty or overflowing
    /// ranges.
    pub fn new(offset: u64, length: u64) -> Result<Self, AssetRangeError> {
        if length == 0 {
            return Err(AssetRangeError::ZeroLength);
        }
        offset
            .checked_add(length)
            .ok_or(AssetRangeError::OffsetOverflow { offset, length })?;

        Ok(Self { offset, length })
    }

    /// Returns the first byte offset.
    #[must_use]
    pub const fn offset(self) -> u64 {
        self.offset
    }

    /// Returns the requested number of bytes.
    #[must_use]
    pub const fn length(self) -> u64 {
        self.length
    }

    /// Returns the exclusive end offset, or `None` if the value could not be
    /// represented. Construction already guarantees this is `Some`.
    #[must_use]
    pub const fn end_offset(self) -> Option<u64> {
        self.offset.checked_add(self.length)
    }
}

#[cfg(test)]
mod tests {
    use super::AssetByteRange;
    use crate::AssetRangeError;

    #[test]
    fn accepts_non_empty_ranges_and_reports_checked_end() {
        let range = AssetByteRange::new(4, 6).expect("range should be valid");

        assert_eq!(range.offset(), 4);
        assert_eq!(range.length(), 6);
        assert_eq!(range.end_offset(), Some(10));
    }

    #[test]
    fn rejects_empty_and_overflowing_ranges() {
        assert!(matches!(
            AssetByteRange::new(0, 0),
            Err(AssetRangeError::ZeroLength)
        ));
        assert!(matches!(
            AssetByteRange::new(u64::MAX, 1),
            Err(AssetRangeError::OffsetOverflow { .. })
        ));
        assert!(matches!(
            AssetByteRange::new(u64::MAX - 1, 3),
            Err(AssetRangeError::OffsetOverflow { .. })
        ));
    }
}
