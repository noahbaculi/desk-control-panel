use core::ops::{Div, Mul};
use defmt::Format;
use embassy_time::Duration;
use num_traits::{PrimInt, Unsigned};
use serde::{Deserialize, Serialize};

const UART_INTERVAL_MS: u64 = 500;
pub const UART_COMMUNICATION_INTERVAL: Duration = Duration::from_millis(UART_INTERVAL_MS);
pub const UART_COMMUNICATION_TIMEOUT: Duration = Duration::from_millis(4 * UART_INTERVAL_MS);

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Format)]
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

        let scaled = numerator
            .to_u64()?
            .saturating_mul(u8::MAX as u64)
            .saturating_div(denominator.to_u64()?) as u8;
        Some(Self(scaled))
    }

    /// Apply the ratio to a value of arbitrary unsigned integer type
    pub fn apply_to<T>(&self, value: T) -> T
    where
        T: Mul<usize, Output = T> + Div<usize, Output = T>,
    {
        value * self.0 as usize / u8::MAX as usize
    }

    /// Create a ProgressRatio from two `Duration`s
    pub fn from_durations(numerator: &Duration, denominator: &Duration) -> Option<Self> {
        let num = numerator.as_millis();
        let denom = denominator.as_millis();

        Self::from_values(num, denom)
    }
}

/// Instructions sent to meeting sign over UART
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Format)]
pub enum MeetingSignInstruction {
    On(ProgressRatio),
    Off,
    Error,
}

// Max postcard serialized size
pub const MAX_PAYLOAD_SIZE: usize = 4;
// Max COBS encoded size
pub const MAX_ENCODED_SIZE: usize = MAX_PAYLOAD_SIZE + (MAX_PAYLOAD_SIZE / 254) + 1;
// Buffer size to contain multiple encoded payloads
pub const RX_BUFFER_SIZE: usize = MAX_ENCODED_SIZE * 8;

pub const FIFO_THRESHOLD: usize = MAX_ENCODED_SIZE * 2;

// Standard COBS delimiter byte
pub const COBS_DELIMITER: u8 = 0x00;
