#![no_std]
#![no_main]

#[cfg(test)]
#[embedded_test::tests(executor = esp_hal_embassy::Executor::new())]
mod tests {
    use defmt::assert_eq;
    use desk_control_panel::{MeetingSignInstruction, MAX_PAYLOAD_SIZE};
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
    async fn test_serialize_all_variants() {
        defmt::info!("Testing serialization of all MeetingSignInstruction variants");

        let mut buf = [0u8; MAX_PAYLOAD_SIZE];

        // Test Duration variant with different values
        for minutes in [1, 60, 180, u8::MAX] {
            let instruction = MeetingSignInstruction::Duration(minutes);
            let result = postcard::to_slice(&instruction, &mut buf);
            assert!(
                result.is_ok(),
                "Failed to serialize Duration({}) - {}",
                minutes,
                result.unwrap_err()
            );
            let serialized_length = result.unwrap().len();
            assert!(serialized_length <= MAX_PAYLOAD_SIZE);

            defmt::info!(
                "Duration({}) serialized to {} bytes",
                minutes,
                serialized_length
            );
        }

        // Test Off variant
        let off_instruction = MeetingSignInstruction::Off;
        let result = postcard::to_slice(&off_instruction, &mut buf);
        assert!(result.is_ok(), "Failed to serialize Off");

        // Test Diagnostic variant
        let diag_instruction = MeetingSignInstruction::Diagnostic;
        let result = postcard::to_slice(&diag_instruction, &mut buf);
        assert!(result.is_ok(), "Failed to serialize Diagnostic");
    }
}
