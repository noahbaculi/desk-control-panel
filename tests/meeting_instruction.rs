#![no_std]
#![no_main]

#[cfg(test)]
#[embedded_test::tests(executor = esp_hal_embassy::Executor::new())]
mod tests {
    use defmt::{assert, info};
    use desk_control_panel::{
        meeting_duration::MeetingDurationError,
        meeting_instruction::{
            MeetingSignInstruction, ProgressRatio, MAX_ENCODED_SIZE, MAX_PAYLOAD_SIZE,
        },
    };
    use esp_hal::timer::systimer::SystemTimer;
    use rtt_target::rtt_init_defmt;

    #[init]
    fn init() {
        let peripherals = esp_hal::init(esp_hal::Config::default());

        let timer0 = SystemTimer::new(peripherals.SYSTIMER);
        esp_hal_embassy::init(timer0.alarm0);

        rtt_init_defmt!();
    }

    #[test]
    fn test_from_values() -> Result<(), MeetingDurationError> {
        assert_eq!(ProgressRatio::from_values(1u8, 2), Some(ProgressRatio(127)));
        assert_eq!(
            ProgressRatio::from_values(50u8, 100),
            Some(ProgressRatio(127))
        );
        assert_eq!(ProgressRatio::from_values(0u8, 0), None);
        assert_eq!(ProgressRatio::from_values(3u8, 4), Some(ProgressRatio(191)));
        assert_eq!(
            ProgressRatio::from_values(123u8, u8::MAX),
            Some(ProgressRatio(123))
        );

        Ok(())
    }

    #[test]
    fn test_serialize_and_encoding_round_trip() -> Result<(), MeetingDurationError> {
        info!(
            "Testing serialization and encoding round-trip of all MeetingSignInstruction variants"
        );

        let mut serialize_buf = [0u8; MAX_PAYLOAD_SIZE];
        let mut encode_buf = [0u8; MAX_ENCODED_SIZE];
        let mut decode_buf = [0u8; MAX_PAYLOAD_SIZE];

        // Test all instruction variants
        let instructions = [
            MeetingSignInstruction::Off,
            MeetingSignInstruction::On(ProgressRatio(0)),
            MeetingSignInstruction::On(ProgressRatio(85)),
            MeetingSignInstruction::On(ProgressRatio(127)),
            MeetingSignInstruction::On(ProgressRatio(u8::MAX)),
        ];

        for (idx, orig_instruction) in instructions.iter().enumerate() {
            info!("Testing instruction {}: {:?}", idx, orig_instruction);

            // Step 1: Serialize
            let serialized =
                postcard::to_slice(orig_instruction, &mut serialize_buf).map_err(|e| {
                    panic!(
                        "Failed to serialize instruction {:?}: {}",
                        orig_instruction, e
                    )
                })?;

            let serialized_len = serialized.len();
            assert!(
                serialized_len <= MAX_PAYLOAD_SIZE,
                "Serialized size {} exceeds MAX_PAYLOAD_SIZE {} for instruction {:?}",
                serialized_len,
                MAX_PAYLOAD_SIZE,
                orig_instruction
            );

            info!("  Serialized to {} bytes: {:?}", serialized_len, serialized);

            // Step 2: COBS encode
            let encoded_len = cobs::encode(serialized, &mut encode_buf);
            assert!(
                encoded_len <= MAX_ENCODED_SIZE,
                "Encoded size {} exceeds MAX_ENCODED_SIZE {} for instruction {:?}",
                encoded_len,
                MAX_ENCODED_SIZE,
                orig_instruction
            );

            // Verify COBS encoding doesn't contain null bytes
            let encoded_data = &encode_buf[..encoded_len];
            for (byte_idx, &byte) in encoded_data.iter().enumerate() {
                assert_ne!(
                    byte, 0x00,
                    "COBS encoding failed: null byte found at position {byte_idx} for instruction {orig_instruction:?}"
                );
            }

            info!(
                "  COBS encoded to {} bytes: {:?}",
                encoded_len, encoded_data
            );

            // Step 3: COBS decode
            let decoded_len = cobs::decode(encoded_data, &mut decode_buf).map_err(|e| {
                panic!(
                    "Failed to COBS decode instruction {:?}: {:?}",
                    orig_instruction, e
                )
            })?;

            assert_eq!(
                decoded_len, serialized_len,
                "Decoded length {decoded_len} doesn't match original serialized length {serialized_len} for instruction {orig_instruction:?}"
            );

            let decoded_data = &decode_buf[..decoded_len];
            assert_eq!(
                decoded_data, serialized,
                "COBS decode didn't match original serialized data for instruction {orig_instruction:?}"
            );

            info!(
                "  COBS decoded to {} bytes: {:?}",
                decoded_len, decoded_data
            );

            // Step 4: Deserialize
            let deserialized_instruction: MeetingSignInstruction =
                postcard::from_bytes(decoded_data).map_err(|e| {
                    panic!(
                        "Failed to deserialize instruction {:?}: {}",
                        orig_instruction, e
                    )
                })?;

            // Step 5: Verify round-trip
            assert_eq!(
                orig_instruction, &deserialized_instruction,
                "Round-trip failed for instruction {orig_instruction:?}, got {deserialized_instruction:?}"
            );

            info!("  Round-trip successful: {:?}", deserialized_instruction);
            info!("  ✓ Instruction {} passed all tests", idx);
        }

        info!(
            "All {} instruction variants passed serialization and round-trip tests",
            instructions.len()
        );
        Ok(())
    }
}
