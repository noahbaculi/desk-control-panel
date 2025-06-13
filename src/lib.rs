#![no_std]

use defmt::Format;
use serde::{Deserialize, Serialize};

// fifo_full_threshold (RX)
pub const READ_BUF_SIZE: usize = 64;
// EOT (CTRL-D)
pub const AT_CMD: u8 = 0x04;

#[derive(Debug, Serialize, Deserialize, Format)]
pub enum MeetingSignInstruction {
    Duration(u8), // Number of minutes
    Off,
    Diagnostic,
}

pub const MAX_PAYLOAD_SIZE: usize = 8; // Max postcard serialized size
pub const MAX_ENCODED_SIZE: usize = MAX_PAYLOAD_SIZE + (MAX_PAYLOAD_SIZE / 254) + 1;
pub const RX_BUFFER_SIZE: usize = MAX_ENCODED_SIZE * 2; // Buffer multiple messages
