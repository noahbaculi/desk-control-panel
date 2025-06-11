#![no_std]

use serde::{Deserialize, Serialize};

// fifo_full_threshold (RX)
pub const READ_BUF_SIZE: usize = 64;
// EOT (CTRL-D)
pub const AT_CMD: u8 = 0x04;

#[derive(Debug, Serialize, Deserialize)]
pub enum MeetingSignInstruction {
    Duration(u8), // Number of minutes
    Off,
    Diagnostic,
}
