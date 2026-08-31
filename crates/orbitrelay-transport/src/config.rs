//! Transport-wide limits independent of any concrete network adapter.

use serde::{Deserialize, Serialize};

use crate::TransportConfigError;

const DEFAULT_OUTBOUND_CAPACITY: usize = 256;
const DEFAULT_MAX_MESSAGE_BYTES: usize = 1024 * 1024;
const DEFAULT_HEARTBEAT_INTERVAL_MILLISECONDS: u64 = 30_000;
const DEFAULT_NEGOTIATION_TIMEOUT_MILLISECONDS: u64 = 10_000;

/// Validated limits shared by transport adapters.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransportConfig {
    outbound_capacity: usize,
    max_message_bytes: usize,
    heartbeat_interval_milliseconds: u64,
    negotiation_timeout_milliseconds: u64,
}

impl TransportConfig {
    /// Creates transport configuration from explicit values.
    #[must_use]
    pub const fn new(
        outbound_capacity: usize,
        max_message_bytes: usize,
        heartbeat_interval_milliseconds: u64,
        negotiation_timeout_milliseconds: u64,
    ) -> Self {
        Self {
            outbound_capacity,
            max_message_bytes,
            heartbeat_interval_milliseconds,
            negotiation_timeout_milliseconds,
        }
    }

    /// Validates all configured limits.
    pub fn validate(&self) -> Result<(), TransportConfigError> {
        require_non_zero(self.outbound_capacity, "outbound_capacity")?;
        require_non_zero(self.max_message_bytes, "max_message_bytes")?;
        require_non_zero(
            self.heartbeat_interval_milliseconds,
            "heartbeat_interval_milliseconds",
        )?;
        require_non_zero(
            self.negotiation_timeout_milliseconds,
            "negotiation_timeout_milliseconds",
        )?;
        Ok(())
    }

    /// Returns the maximum number of queued outbound messages per connection.
    #[must_use]
    pub const fn outbound_capacity(&self) -> usize {
        self.outbound_capacity
    }

    /// Returns the maximum accepted encoded message size in bytes.
    #[must_use]
    pub const fn max_message_bytes(&self) -> usize {
        self.max_message_bytes
    }

    /// Returns the heartbeat interval in milliseconds.
    #[must_use]
    pub const fn heartbeat_interval_milliseconds(&self) -> u64 {
        self.heartbeat_interval_milliseconds
    }

    /// Returns the negotiation timeout in milliseconds.
    #[must_use]
    pub const fn negotiation_timeout_milliseconds(&self) -> u64 {
        self.negotiation_timeout_milliseconds
    }
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self::new(
            DEFAULT_OUTBOUND_CAPACITY,
            DEFAULT_MAX_MESSAGE_BYTES,
            DEFAULT_HEARTBEAT_INTERVAL_MILLISECONDS,
            DEFAULT_NEGOTIATION_TIMEOUT_MILLISECONDS,
        )
    }
}

fn require_non_zero<T>(value: T, field: &'static str) -> Result<(), TransportConfigError>
where
    T: Default + PartialEq,
{
    if value == T::default() {
        Err(TransportConfigError::NonZeroRequired { field })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::TransportConfig;

    #[test]
    fn default_config_is_valid() {
        let config = TransportConfig::default();

        assert!(config.validate().is_ok());
        assert!(config.outbound_capacity() > 0);
        assert!(config.max_message_bytes() > 0);
    }

    #[test]
    fn rejects_zero_values() {
        let config = TransportConfig::new(0, 1024, 30_000, 10_000);

        assert!(config.validate().is_err());
    }
}
