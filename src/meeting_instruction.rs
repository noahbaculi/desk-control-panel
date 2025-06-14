#![allow(dead_code)]

use defmt::Format;
use serde::{Deserialize, Serialize};

use crate::meeting_duration::MeetingDuration;

// fifo_full_threshold (RX)
pub const READ_BUF_SIZE: usize = 64;
// EOT (CTRL-D)
pub const AT_CMD: u8 = 0x04;

/// Validated time duration in quarter-second units for UART communication.
///
/// This wrapper ensures durations are properly validated before being sent over UART.
/// Cannot be constructed directly - must be created from a `MeetingDuration`.
#[derive(Debug, Serialize, Deserialize, Format)]
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

/// Instructions sent to meeting sign.
#[derive(Debug, Serialize, Deserialize, Format)]
pub enum MeetingSignInstruction {
    Duration(QuarterSeconds),
    Off,
    Diagnostic,
}

impl From<MeetingDuration> for MeetingSignInstruction {
    fn from(value: MeetingDuration) -> Self {
        Self::Duration(value.into())
    }
}

// Max postcard serialized size
pub const MAX_PAYLOAD_SIZE: usize = 4;
// Max COBS encoded size
pub const MAX_ENCODED_SIZE: usize = MAX_PAYLOAD_SIZE + (MAX_PAYLOAD_SIZE / 254) + 1;
// Buffer size to contain multiple encoded payloads
pub const RX_BUFFER_SIZE: usize = MAX_ENCODED_SIZE * 4;
