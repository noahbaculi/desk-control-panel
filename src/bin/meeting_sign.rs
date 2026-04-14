#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use desk_control_panel::meeting_instruction::{
    self, MeetingSignInstruction, ProgressRatio, COBS_DELIMITER, UART_COMMUNICATION_TIMEOUT,
};
use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::pubsub::{ImmediatePublisher, PubSubChannel, Subscriber};
use embassy_time::{Duration, Instant, Timer};
use esp_hal::rtc_cntl::Rtc;
use esp_hal::{
    clock::CpuClock,
    gpio::{AnyPin, Level, Output, OutputConfig},
    interrupt::software::SoftwareInterruptControl,
    timer::systimer::SystemTimer,
    uart::{Config, RxConfig, Uart},
    Async,
};
use log::{debug, error, info, trace, warn, LevelFilter};
use static_cell::StaticCell;

const NUM_LEDS: usize = 9;
const LED_PINS: [u8; NUM_LEDS] = [5, 6, 7, 3, 4, 10, 20, 21, 0];
const STATUS_GPIO_PIN_NUMBER: u32 = 1;

const BUILT_IN_TIMER_DURATION: Duration = Duration::from_secs(60 * 90); // 90 minutes
const BUILT_IN_TIMER_UPDATE_INTERVAL: Duration = Duration::from_secs(60 * 5);

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

// Global static for the panic pin
static mut STATUS_PIN: Option<Output<'static>> = None;

type LEDsMutex = Mutex<CriticalSectionRawMutex, LEDs<'static>>;
static LEDS: StaticCell<LEDsMutex> = StaticCell::new();

#[derive(Clone)]
enum MeetingSignState {
    NoUart,
    Uart(MeetingSignInstruction),
}
const STATE_PUB_SUB_CAPACITY: usize = 1;
const STATE_NUM_PUBLISHERS: usize = 0;
const STATE_NUM_SUBSCRIBERS: usize = 2;
type MeetingSignStatePubSubChannel = PubSubChannel<
    CriticalSectionRawMutex,
    MeetingSignState,
    STATE_PUB_SUB_CAPACITY,
    STATE_NUM_SUBSCRIBERS,
    STATE_NUM_PUBLISHERS,
>;
type MeetingSignStatePublisher<'a> = ImmediatePublisher<
    'a,
    CriticalSectionRawMutex,
    MeetingSignState,
    STATE_PUB_SUB_CAPACITY,
    STATE_NUM_SUBSCRIBERS,
    STATE_NUM_PUBLISHERS,
>;
type MeetingSignStateSubscriber<'a> = Subscriber<
    'a,
    CriticalSectionRawMutex,
    MeetingSignState,
    STATE_PUB_SUB_CAPACITY,
    STATE_NUM_SUBSCRIBERS,
    STATE_NUM_PUBLISHERS,
>;
static MEETING_SIGN_STATE: StaticCell<MeetingSignStatePubSubChannel> = StaticCell::new();

#[esp_rtos::main]
async fn main(spawner: Spawner) {
    esp_println::logger::init_logger(LevelFilter::Debug);

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    let mut rtc = esp_hal::rtc_cntl::Rtc::new(peripherals.LPWR);

    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timer0.alarm0, sw_int.software_interrupt0);
    info!("Embassy initialized!");

    let startup_instant = Instant::now();

    // WARN: This status pin needs to match the emergency hardcoded panic pin
    let status_pin = Output::new(peripherals.GPIO1, Level::Low, OutputConfig::default());
    // Store the pin globally for panic handler access
    unsafe {
        STATUS_PIN = Some(status_pin);
    }

    // WARN: These LED pins need to match the emergency hardcoded LED pins
    let led_pins: [AnyPin; NUM_LEDS] = [
        peripherals.GPIO5.into(),
        peripherals.GPIO6.into(),
        peripherals.GPIO7.into(),
        peripherals.GPIO3.into(),
        peripherals.GPIO4.into(),
        peripherals.GPIO10.into(),
        peripherals.GPIO20.into(),
        peripherals.GPIO21.into(),
        peripherals.GPIO0.into(),
    ];
    let leds = LEDS.init(Mutex::new(LEDs::new(led_pins)));

    // Initialize the LEDs to display the built-in timer
    leds.lock()
        .await
        .display_builtin_timer(&startup_instant, &mut rtc);

    let meeting_sign_state = MEETING_SIGN_STATE.init(MeetingSignStatePubSubChannel::new());

    // We only care about the latest state, so we use immediate publishers
    let state_publisher_1 = meeting_sign_state.immediate_publisher();
    let state_publisher_2 = meeting_sign_state.immediate_publisher();

    // Initialize the state to NoUart
    state_publisher_1.publish_immediate(MeetingSignState::NoUart);

    let state_subscriber_1 = meeting_sign_state
        .subscriber()
        .expect("Failed to create state subscriber 1");
    let mut state_subscriber_2 = meeting_sign_state
        .subscriber()
        .expect("Failed to create state subscriber 2");

    let rx_pin = peripherals.GPIO2;
    let uart_config = Config::default().with_rx(
        RxConfig::default().with_fifo_full_threshold(meeting_instruction::FIFO_THRESHOLD as u16),
    );
    let uart = Uart::new(peripherals.UART0, uart_config)
        .unwrap()
        .with_rx(rx_pin)
        .into_async();

    spawner.spawn(uart_reader(uart, state_publisher_1)).ok();
    spawner
        .spawn(uart_timeout_monitor(state_publisher_2, state_subscriber_1))
        .ok();

    let mut loop_timeout_duration = UART_COMMUNICATION_TIMEOUT;
    let mut last_progress_ratio = ProgressRatio(0);

    // Main loop
    loop {
        match select(
            state_subscriber_2.next_message_pure(),
            Timer::after(loop_timeout_duration),
        )
        .await
        {
            Either::First(state) => match state {
                MeetingSignState::NoUart => {
                    info!("State changed to NoUart.");

                    // Change timeout since we have not received UART commands
                    loop_timeout_duration = BUILT_IN_TIMER_UPDATE_INTERVAL;

                    leds.lock()
                        .await
                        .display_builtin_timer(&startup_instant, &mut rtc);
                }
                MeetingSignState::Uart(instruction) => {
                    info!("State changed to Uart.");
                    match instruction {
                        MeetingSignInstruction::On(progress_ratio) => {
                            // Only update if the ratio has changed
                            if progress_ratio != last_progress_ratio {
                                leds.lock().await.set_ratio_low(&progress_ratio);
                                last_progress_ratio = progress_ratio;
                            }
                        }
                        MeetingSignInstruction::Off => {
                            leds.lock().await.set_pattern_array(&[false; NUM_LEDS]);
                        }
                        MeetingSignInstruction::Error => {
                            leds.lock().await.set_pattern_array(&[
                                true, false, false, false, false, false, false, false, true,
                            ]);
                        }
                    }

                    // Change timeout since we have received UART commands
                    loop_timeout_duration = UART_COMMUNICATION_TIMEOUT;
                }
            },
            Either::Second(_) => {
                info!(
                    "No state change detected within {}s, displaying LEDs according to builtin timer...",
                    loop_timeout_duration.as_secs()
                );

                // Change timeout since we have not received UART commands
                loop_timeout_duration = BUILT_IN_TIMER_UPDATE_INTERVAL;

                // leds.lock().await.set_pattern_array(&[
                //     false, true, true, false, false, false, true, true, false,
                // ]);
                leds.lock()
                    .await
                    .display_builtin_timer(&startup_instant, &mut rtc);
            }
        }
    }
}

struct LEDs<'a> {
    led_outs: [Output<'a>; NUM_LEDS],
}
impl<'a> LEDs<'a> {
    pub fn new(pins: [AnyPin<'a>; NUM_LEDS]) -> Self {
        // Convert each pin to an Output
        let led_outs = pins.map(|pin| Output::new(pin, Level::Low, OutputConfig::default()));

        Self { led_outs }
    }

    /// Set the portion of LEDs to low based on the given ProgressRatio
    pub fn set_ratio_low(&mut self, ratio: &ProgressRatio) {
        let num_on_leds = ratio.apply_to(NUM_LEDS);

        for (led_idx, led) in self.led_outs.iter_mut().enumerate() {
            if led_idx < num_on_leds {
                led.set_low();
            } else {
                led.set_high();
            }
        }
    }

    pub fn set_pattern_array(&mut self, pattern: &[bool; NUM_LEDS]) {
        for (led, &should_be_on) in self.led_outs.iter_mut().zip(pattern.iter()) {
            if should_be_on {
                led.set_high();
            } else {
                led.set_low();
            }
        }
    }

    pub fn display_builtin_timer(&mut self, startup_instant: &Instant, rtc: &mut Rtc) {
        let on_duration = startup_instant.elapsed();
        if on_duration >= BUILT_IN_TIMER_DURATION {
            // If the timer has expired, turn off all LEDs
            for led in self.led_outs.iter_mut() {
                led.set_low();
            }

            // Set the status pin low
            unsafe {
                if let Some(ref mut pin) = STATUS_PIN {
                    pin.set_low();
                } else {
                    emergency_gpio1_high();
                }
            }

            rtc.sleep_deep(&[]);
        } else {
            // Calculate the portion of time elapsed
            debug!(
                "Calculating ratio = {}s / {}s",
                on_duration.as_secs(),
                BUILT_IN_TIMER_DURATION.as_secs()
            );
            let ratio = ProgressRatio::from_durations(&on_duration, &BUILT_IN_TIMER_DURATION)
                .expect("Failed to calculate ratio from durations");
            debug!("Setting LEDs based on ratio: {ratio:?}");
            // Set LEDs based on the portion
            self.set_ratio_low(&ratio);
        }
    }
}

#[embassy_executor::task]
async fn uart_reader(
    mut uart: Uart<'static, Async>,
    state_publisher: MeetingSignStatePublisher<'static>,
) {
    debug!("Starting UART reader task");

    // Buffers sized appropriately for the MeetingInstruction payload
    let mut read_buf = [0u8; meeting_instruction::RX_BUFFER_SIZE];
    let mut decode_buf = [0u8; meeting_instruction::MAX_ENCODED_SIZE];

    let mut offset = 0;
    loop {
        trace!("Beginning read loop, current offset: {offset}");
        match embedded_io_async::Read::read(&mut uart, &mut read_buf[offset..]).await {
            Ok(len) => {
                offset += len;
                trace!("Read {len} bytes, total buffer: {offset}");

                // Look for delimiter
                if let Some(delimiter_pos) =
                    read_buf[..offset].iter().position(|&b| b == COBS_DELIMITER)
                {
                    let encoded_data = &read_buf[..delimiter_pos];
                    trace!("Received encoded data: {encoded_data:?}");

                    match cobs::decode(encoded_data, &mut decode_buf) {
                        Ok(decoded_len) => {
                            let decoded_data = &decode_buf[..decoded_len];
                            trace!("Decoded data: {decoded_data:?}");
                            match postcard::from_bytes::<MeetingSignInstruction>(decoded_data) {
                                Ok(instruction) => {
                                    debug!(
                                        "Received: {instruction:?} @ {}ms",
                                        Instant::now().as_millis()
                                    );
                                    state_publisher
                                        .publish_immediate(MeetingSignState::Uart(instruction));
                                }
                                Err(e) => warn!(
                                    "Deserialization error: {e:?} @ {}ms",
                                    Instant::now().as_millis()
                                ),
                            }
                        }
                        Err(e) => {
                            warn!(
                                "COBS decode error: {e:?} @ {}ms",
                                Instant::now().as_millis()
                            );
                        }
                    }
                    offset = 0; // Reset after processing
                }
            }
            Err(e) => error!("UART read error: {e:?}"),
        }

        // Prevent buffer overflow
        if offset >= meeting_instruction::RX_BUFFER_SIZE - 1 {
            warn!(
                "Buffer overflow, resetting @ {}ms",
                Instant::now().as_millis()
            );
            offset = 0;
        }
    }
}

#[embassy_executor::task]
async fn uart_timeout_monitor(
    state_publisher: MeetingSignStatePublisher<'static>,
    mut state_subscriber: MeetingSignStateSubscriber<'static>,
) {
    debug!("Starting UART timeout monitor task");

    loop {
        // Wait for UART state
        loop {
            match state_subscriber.next_message_pure().await {
                MeetingSignState::Uart(_) => break,   // Start monitoring
                MeetingSignState::NoUart => continue, // Keep waiting
            }
        }

        // Now we're in UART state, start the timeout monitoring
        let mut timeout_timer = Timer::after(UART_COMMUNICATION_TIMEOUT);

        loop {
            match select(state_subscriber.next_message_pure(), &mut timeout_timer).await {
                Either::First(state) => {
                    match state {
                        MeetingSignState::Uart(_) => {
                            // New UART message received, reset the timer
                            debug!("UART activity detected, resetting timeout");
                            timeout_timer = Timer::after(UART_COMMUNICATION_TIMEOUT);
                        }
                        MeetingSignState::NoUart => {
                            // Someone else set it to NoUART, stop monitoring
                            debug!("State changed to NoUART, stopping timeout monitoring");
                            break;
                        }
                    }
                }
                Either::Second(_) => {
                    // Timeout occurred
                    debug!("UART communication timed out, signaling NoUART");
                    state_publisher.publish_immediate(MeetingSignState::NoUart);
                    break; // Exit inner loop, will wait for next UART state
                }
            }
        }
    }
}
#[panic_handler]
fn panic_handler(info: &core::panic::PanicInfo) -> ! {
    // Disable interrupts to prevent further issues
    critical_section::with(|_| {
        // Try to set the status pin low
        unsafe {
            if let Some(ref mut pin) = STATUS_PIN {
                pin.set_high();
            } else {
                emergency_gpio1_high();
            }

            set_leds_panic_pattern();
        }
    });

    error!("Panic occurred: {info:?}");

    // Halt the system
    loop {
        core::hint::spin_loop();
    }
}

// ESP32-C3 GPIO register addresses (from ESP32-C3 TRM Table 3.3-3 and Section 5.14.1)
// GPIO base address: 0x60004000, offsets from Section 5.14.1:
// https://www.espressif.com/sites/default/files/documentation/esp32-c3_technical_reference_manual_en.pdf
const GPIO_BASE_REG: u32 = 0x60004000;
const GPIO_OUT_REG: *mut u32 = (GPIO_BASE_REG + 0x0004) as *mut u32; // GPIO output register
const GPIO_ENABLE_REG: *mut u32 = (GPIO_BASE_REG + 0x0020) as *mut u32; // GPIO output enable register

/// Emergency function to set GPIO pin high using direct register access
unsafe fn emergency_gpio1_high() {
    // Enable GPIO pin as output
    let enable_val = core::ptr::read_volatile(GPIO_ENABLE_REG);
    core::ptr::write_volatile(GPIO_ENABLE_REG, enable_val | (1 << STATUS_GPIO_PIN_NUMBER));

    // Set GPIO pin high
    let out_val = core::ptr::read_volatile(GPIO_OUT_REG);
    core::ptr::write_volatile(GPIO_OUT_REG, out_val | (1 << STATUS_GPIO_PIN_NUMBER));
}

/// Panic function to set panic pattern on LEDs via GPIO pins
unsafe fn set_leds_panic_pattern() {
    // Set every other LED high
    for (pin_number, turn_on) in LED_PINS.iter().zip([true, false].iter().cycle()) {
        // Enable GPIO pin as output
        let enable_val = core::ptr::read_volatile(GPIO_ENABLE_REG);
        core::ptr::write_volatile(GPIO_ENABLE_REG, enable_val | (1 << pin_number));

        match turn_on {
            true => {
                // Set GPIO pin high
                let out_val = core::ptr::read_volatile(GPIO_OUT_REG);
                core::ptr::write_volatile(GPIO_OUT_REG, out_val | (1 << pin_number));
            }
            false => {
                // Set GPIO pin low
                let out_val = core::ptr::read_volatile(GPIO_OUT_REG);
                core::ptr::write_volatile(GPIO_OUT_REG, out_val & !(1 << pin_number));
            }
        }
    }
}
