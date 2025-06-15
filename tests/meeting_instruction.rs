#![no_std]
#![no_main]

#[cfg(test)]
#[embedded_test::tests(executor = esp_hal_embassy::Executor::new())]
mod tests {
    use defmt::{assert, info};
    use desk_control_panel::{
        meeting_duration::{MeetingDuration, MeetingDurationError},
        meeting_instruction::{MeetingSignInstruction, MAX_ENCODED_SIZE, MAX_PAYLOAD_SIZE},
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
    fn test_serialize_all_variants() -> Result<(), MeetingDurationError> {
        info!("Testing serialization of all MeetingSignInstruction variants");

        let mut buf = [0u8; MAX_PAYLOAD_SIZE];

        // Test Duration variant with different values
        for instruction in [
            MeetingSignInstruction::Off,
            MeetingSignInstruction::Diagnostic,
            MeetingSignInstruction::from(MeetingDuration::from_minutes(1)?),
            MeetingSignInstruction::from(MeetingDuration::from_minutes(5)?),
            MeetingSignInstruction::from(MeetingDuration::from_minutes(30)?),
            MeetingSignInstruction::from(MeetingDuration::from_minutes(60)?),
            MeetingSignInstruction::from(MeetingDuration::MAX),
        ] {
            let result = postcard::to_slice(&instruction, &mut buf);
            assert!(
                result.is_ok(),
                "Failed to serialize instruction {:?} - {}",
                instruction,
                result.unwrap_err()
            );
            let serialized_length = result.unwrap().len();
            assert!(serialized_length <= MAX_PAYLOAD_SIZE);

            info!(
                "Instruction {:?} serialized to {} bytes",
                instruction, serialized_length
            );
        }

        Ok(())
    }

    #[test]
    fn test_round_trip_serialization() {
        let mut serialize_buf = [0u8; MAX_PAYLOAD_SIZE];
        let mut encode_buf = [0u8; MAX_ENCODED_SIZE];
        let mut decode_buf = [0u8; MAX_PAYLOAD_SIZE];

        // Test Duration instruction round-trip
        let original_duration = MeetingDuration::from_minutes(45).unwrap();
        let original_instruction = MeetingSignInstruction::from(original_duration);

        // Serialize
        let serialized = postcard::to_slice(&original_instruction, &mut serialize_buf).unwrap();

        // COBS encode
        let encoded_len = cobs::encode(serialized, &mut encode_buf);

        // COBS decode
        let decoded_len = cobs::decode(&encode_buf[..encoded_len], &mut decode_buf).unwrap();

        // Deserialize
        let decoded_instruction: MeetingSignInstruction =
            postcard::from_bytes(&decode_buf[..decoded_len]).unwrap();

        // Verify round-trip
        assert_eq!(original_instruction, decoded_instruction);
    }
}
