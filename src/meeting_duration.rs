#![allow(dead_code)]

use defmt::Format;
use embassy_time::Duration;
use thiserror::Error;

/// A validated duration for meeting sign operations.
///
/// Wraps embassy_time::Duration with domain-specific validation and constraints.
/// Ensures durations stay within reasonable bounds for meeting sign usage.
#[derive(Debug, Copy, Clone, Format)]
pub struct MeetingDuration(Duration);

#[derive(Debug, Error, Format)]
pub enum MeetingDurationError {
    #[error("Duration {seconds} seconds exceeds maximum of {max_seconds} seconds")]
    TooLong { seconds: u64, max_seconds: u16 },
}

impl MeetingDuration {
    const MAX_NUM_HOURS: u16 = 2;
    const MAX_NUM_SECS: u16 = 60 * 60 * Self::MAX_NUM_HOURS;
    const MAX_DURATION: Duration = Duration::from_secs(Self::MAX_NUM_SECS as u64);
    pub const MAX: Self = Self(Self::MAX_DURATION);

    // Compile-time assertion to ensure MAX_DURATION fits in QuarterSeconds (u16)
    // Maximum MeetingDuration therefore has an upper bound of `u16::MAX / 60 / 60 / 4` = 4.55 hrs
    const _ASSERT_CONVERTS_TO_QUARTER_SECONDS: u16 = Self::MAX_NUM_SECS * 4; // seconds * 4 = quarter-seconds

    /// Get the wrapped Duration
    pub fn as_duration(&self) -> Duration {
        self.0
    }

    /// Create a new MeetingDuration with validation
    pub fn new(duration: Duration) -> Result<Self, MeetingDurationError> {
        if duration > Self::MAX_DURATION {
            return Err(MeetingDurationError::TooLong {
                seconds: duration.as_secs(),
                max_seconds: Self::MAX_NUM_SECS,
            });
        }

        Ok(Self(duration))
    }

    pub fn from_minutes(minutes: u8) -> Result<Self, MeetingDurationError> {
        let duration = Duration::from_secs(u64::from(minutes) * 60);
        Self::new(duration)
    }

    /// Convert to quarter-seconds for UART communication
    pub fn to_uart_quarter_seconds(&self) -> u16 {
        let total_millis = self.0.as_millis();
        let quarter_seconds = total_millis * 4 / 1000;

        debug_assert!(quarter_seconds < u16::MAX.into());

        quarter_seconds as u16
    }

    /// Create from UART quarter-seconds value
    pub fn from_uart_quarter_seconds(quarter_seconds: u16) -> Result<Self, MeetingDurationError> {
        let duration = Duration::from_millis((quarter_seconds as u64) * 1000 / 4);
        Self::new(duration)
    }

    /// Add the specified number of minutes with validation
    pub fn add_minutes(&self, minutes: u8) -> Result<Self, MeetingDurationError> {
        let additional = Duration::from_secs(u64::from(minutes) * 60);
        let new_duration = self.0 + additional;
        Self::new(new_duration)
    }
}

// Convenient conversion from Duration (with validation)
impl TryFrom<Duration> for MeetingDuration {
    type Error = MeetingDurationError;

    fn try_from(duration: Duration) -> Result<Self, Self::Error> {
        Self::new(duration)
    }
}

impl From<MeetingDuration> for Duration {
    fn from(meeting_duration: MeetingDuration) -> Self {
        meeting_duration.0
    }
}

// Easy access to the inner Duration
impl AsRef<Duration> for MeetingDuration {
    fn as_ref(&self) -> &Duration {
        &self.0
    }
}

#[cfg(test)]
#[embedded_test::tests(executor = esp_hal_embassy::Executor::new())]
mod tests {
    use super::*;
    use defmt::assert;
    use defmt::assert_eq;

    #[test]
    fn test_valid_duration() {
        let duration = MeetingDuration::from_minutes(60).unwrap();
        assert_eq!(duration.as_duration(), Duration::from_secs(3600));
    }

    #[test]
    fn test_too_long() {
        let result = MeetingDuration::from_minutes(600); // 10 hours
        assert!(result.is_err());
    }

    #[test]
    fn test_uart_conversion() {
        let duration = MeetingDuration::from_minutes(30).unwrap();
        let quarter_secs = duration.to_uart_quarter_seconds();
        assert_eq!(quarter_secs, 30 * 60 * 4); // 30 minutes in quarter seconds

        // Round trip
        let reconstructed = MeetingDuration::from_uart_quarter_seconds(quarter_secs).unwrap();
        assert_eq!(duration.as_duration(), reconstructed.as_duration());
    }
}
