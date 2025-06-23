#![allow(dead_code)]

use crate::meeting_duration::MeetingDuration;
use defmt::Format;
use embassy_time::Duration;
use serde::{Deserialize, Serialize};

// fifo_full_threshold (RX)
pub const READ_BUF_SIZE: usize = 64;
// EOT (CTRL-D)
pub const AT_CMD: u8 = 0x04;

pub const UART_COMMUNICATION_INTERVAL: Duration = Duration::from_secs(1);
pub const UART_COMMUNICATION_TIMEOUT: Duration = Duration::from_secs(4);

/// Validated time duration in quarter-second units for UART communication.
///
/// This wrapper ensures durations are properly validated before being sent over UART.
/// Cannot be constructed directly - must be created from a `MeetingDuration`.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Format)]
pub struct QuarterSeconds(u16);

impl QuarterSeconds {
    /// Create a new QuarterSeconds value (crate-internal use only).
    pub(crate) fn new(quarter_seconds: u16) -> Self {
        Self(quarter_seconds)
    }

    /// Get the raw quarter-seconds value.
    pub fn get(&self) -> u16 {
        self.0
    }
}

impl From<MeetingDuration> for QuarterSeconds {
    /// Convert a validated MeetingDuration to quarter-seconds for UART transmission.
    ///
    /// # Panics
    /// Panics if the duration cannot be represented as quarter-seconds. This should not happen
    /// with validated MeetingDuration instances.
    fn from(value: MeetingDuration) -> Self {
        Self(value.to_uart_quarter_seconds())
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Format)]
#[serde(transparent)]
pub struct ProgressRatio(pub u8);
impl ProgressRatio {
    fn from_values(numerator: u8, denominator: u8) -> Option<Self> {
        if denominator == 0 {
            return None; // Avoid division by zero
        }
        if numerator > denominator {
            return None; // Avoid ratios greater than 1
        }

        let ratio = ((numerator as u16) * (u8::MAX as u16) / denominator as u16) as u8;
        Some(Self(ratio))
    }

    fn apply_to_value(&self, value: u16) -> u16 {
        // Apply the progress ratio to a value
        (value as u32 * self.0 as u32 / u8::MAX as u32) as u16
    }
}

/// Instructions sent to meeting sign over UART
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Format)]
pub enum MeetingSignInstruction {
    On(ProgressRatio),
    Off,
    Diagnostic,
}

// Max postcard serialized size
pub const MAX_PAYLOAD_SIZE: usize = 4;
// Max COBS encoded size
pub const MAX_ENCODED_SIZE: usize = MAX_PAYLOAD_SIZE + (MAX_PAYLOAD_SIZE / 254) + 1;
// Buffer size to contain multiple encoded payloads
pub const RX_BUFFER_SIZE: usize = MAX_ENCODED_SIZE * 4;
