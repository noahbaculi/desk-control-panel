#![no_std]
#![no_main]

#[cfg(test)]
#[embedded_test::tests(executor = esp_hal_embassy::Executor::new())]
mod tests {
    use defmt::assert;
    use desk_control_panel::{
        meeting_duration::{MeetingDuration, MeetingDurationError},
        meeting_instruction::{MeetingSignInstruction, MAX_PAYLOAD_SIZE},
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
    async fn test_serialize_all_variants() -> Result<(), MeetingDurationError> {
        defmt::info!("Testing serialization of all MeetingSignInstruction variants");

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

            defmt::info!(
                "Instruction {:?} serialized to {} bytes",
                instruction,
                serialized_length
            );
        }

        Ok(())
    }
}
