#![allow(dead_code)]

use crate::meeting_duration::MeetingDuration;
use core::ops::{Div, Mul};
use defmt::Format;
use embassy_time::Duration;
use num_traits::{PrimInt, Unsigned};
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
    /// Create a ratio from `numerator / denominator`, scaled to 0..=255
    pub fn from_values<T>(numerator: T, denominator: T) -> Option<Self>
    where
        T: PrimInt + Unsigned,
    {
        if denominator.is_zero() {
            return None;
        }

        let scaled = (numerator.to_u32()? * u8::MAX as u32) / denominator.to_u32()?;
        Some(Self(scaled.min(u8::MAX as u32) as u8))
    }

    /// Apply the ratio to a value of arbitrary unsigned integer type
    pub fn apply_to<T>(&self, value: T) -> T
    where
        T: Mul<u32, Output = T> + Div<u32, Output = T>,
    {
        value * self.0 as u32 / u8::MAX as u32
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
