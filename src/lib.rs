#![no_std]

use defmt::Format;
use serde::{Deserialize, Serialize};

// fifo_full_threshold (RX)
pub const READ_BUF_SIZE: usize = 64;
// EOT (CTRL-D)
pub const AT_CMD: u8 = 0x04;

#[derive(Debug, Copy, Clone, Serialize, Deserialize, Format)]
pub struct Minutes(pub u8);
impl Minutes {
    pub const MIN: Minutes = Minutes(0);

    /// Maximum for minutes (3 hours)
    pub const MAX: Minutes = Minutes(3 * 60);
}

/// A duration represented in quarter-seconds (0.25s units).
///
/// Internally stored as a `u16`, allowing a maximum representable duration of
/// 65,535 quarter-seconds (approximately 4.55 hours).
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Format)]
pub struct QuarterSeconds(pub u16);
impl QuarterSeconds {
    pub fn from_minutes(minutes: Minutes) -> Self {
        Self(minutes.0 as u16 * 4)
    }
}

#[derive(Debug, Serialize, Deserialize, Format)]
pub enum MeetingSignInstruction {
    Duration(QuarterSeconds), // Number of quarter seconds
    Off,
    Diagnostic,
}

pub const MAX_PAYLOAD_SIZE: usize = 3; // Max postcard serialized size
pub const MAX_ENCODED_SIZE: usize = MAX_PAYLOAD_SIZE + (MAX_PAYLOAD_SIZE / 254) + 1;
pub const RX_BUFFER_SIZE: usize = MAX_ENCODED_SIZE * 2; // Buffer multiple messages
