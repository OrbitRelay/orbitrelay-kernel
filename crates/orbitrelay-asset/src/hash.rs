//! Validated SHA-256 content hashes.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::AssetError;

/// A canonical lowercase hexadecimal SHA-256 digest.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ContentHash(String);

impl ContentHash {
    /// Parses a 64-character hexadecimal SHA-256 digest.
    ///
    /// Uppercase hexadecimal input is accepted and normalized to lowercase so
    /// the stored representation is canonical.
    pub fn parse(value: &str) -> Result<Self, AssetError> {
        if value.len() != 64 {
            return Err(AssetError::InvalidContentHash {
                reason: "SHA-256 digest must contain 64 hexadecimal characters",
            });
        }

        let mut canonical = String::with_capacity(64);
        for byte in value.bytes() {
            if !byte.is_ascii_hexdigit() {
                return Err(AssetError::InvalidContentHash {
                    reason: "SHA-256 digest contains a non-hexadecimal character",
                });
            }
            canonical.push(byte.to_ascii_lowercase() as char);
        }

        Ok(Self(canonical))
    }

    /// Creates a hash from its exact 32-byte digest representation.
    #[must_use]
    pub fn from_bytes(value: [u8; 32]) -> Self {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut canonical = String::with_capacity(64);
        for byte in value {
            canonical.push(HEX[(byte >> 4) as usize] as char);
            canonical.push(HEX[(byte & 0x0f) as usize] as char);
        }
        Self(canonical)
    }

    /// Returns the canonical lowercase hexadecimal digest.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for ContentHash {
    type Err = AssetError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Serialize for ContentHash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ContentHash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::ContentHash;

    #[test]
    fn normalizes_uppercase_hex_to_canonical_lowercase() {
        let hash = ContentHash::parse(&"AB".repeat(32)).expect("hash should be valid");

        assert_eq!(hash.as_str(), &"ab".repeat(32));
    }

    #[test]
    fn creates_canonical_hash_from_bytes() {
        let hash = ContentHash::from_bytes([0xab; 32]);

        assert_eq!(hash.as_str(), &"ab".repeat(32));
    }

    #[test]
    fn rejects_wrong_length_and_non_hex_values() {
        assert!(ContentHash::parse("00").is_err());
        assert!(ContentHash::parse(&format!("{}g", "0".repeat(63))).is_err());
    }

    #[test]
    fn round_trips_as_a_canonical_json_string() {
        let hash = ContentHash::parse(&"CD".repeat(32)).expect("hash should be valid");
        let encoded = serde_json::to_string(&hash).expect("hash should serialize");
        let decoded: ContentHash = serde_json::from_str(&encoded).expect("hash should decode");

        assert_eq!(encoded, format!("\"{}\"", "cd".repeat(32)));
        assert_eq!(decoded, hash);
    }
}
