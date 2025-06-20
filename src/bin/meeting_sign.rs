#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use desk_control_panel::meeting_instruction::{
    self, MeetingSignInstruction, UART_COMMUNICATION_TIMEOUT,
};
use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::pubsub::{ImmediatePublisher, PubSubChannel, Subscriber};
use embassy_time::Timer;
use esp_hal::{
    clock::CpuClock,
    gpio::{AnyPin, Level, Output, OutputConfig},
    timer::systimer::SystemTimer,
    uart::{Config, RxConfig, Uart},
    Async,
};
use log::{debug, error, info, trace, warn, LevelFilter};
use micromath::F32Ext;
use static_cell::StaticCell;

const NUM_LEDS: usize = 9;
const LED_PINS: [u8; NUM_LEDS] = [5, 6, 7, 8, 9, 10, 20, 21, 0];

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

// Global static for the panic pin
static mut PANIC_PIN: Option<Output<'static>> = None;

#[derive(Clone)]
enum MeetingSignState {
    NoUart,
    Uart,
}

type LEDsMutex = Mutex<CriticalSectionRawMutex, LEDs<'static>>;
static LEDS: StaticCell<LEDsMutex> = StaticCell::new();

const STATE_PUB_SUB_CAPACITY: usize = 1;
const STATE_NUM_PUBLISHERS: usize = 0;
const STATE_NUM_SUBSCRIBERS: usize = 3;
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

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    esp_println::logger::init_logger(LevelFilter::Debug);

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);
    info!("Embassy initialized!");

    // WARN: This panic pin needs to match the emergency hardcoded panic pin
    let panic_pin = Output::new(peripherals.GPIO3, Level::High, OutputConfig::default());
    // Store the pin globally for panic handler access
    unsafe {
        PANIC_PIN = Some(panic_pin);
    }

    // WARN: These LED pins need to match the emergency hardcoded LED pins
    let led_pins: [AnyPin; NUM_LEDS] = [
        peripherals.GPIO5.into(),
        peripherals.GPIO6.into(),
        peripherals.GPIO7.into(),
        peripherals.GPIO8.into(),
        peripherals.GPIO9.into(),
        peripherals.GPIO10.into(),
        peripherals.GPIO20.into(),
        peripherals.GPIO21.into(),
        peripherals.GPIO0.into(),
    ];
    let leds = LEDS.init(Mutex::new(LEDs::new(led_pins)));

    let meeting_sign_state = MEETING_SIGN_STATE.init(MeetingSignStatePubSubChannel::new());

    // We only care about the latest state, so we use immediate publishers
    let state_publisher_1 = meeting_sign_state.immediate_publisher();
    let state_publisher_2 = meeting_sign_state.immediate_publisher();

    // Initialize the state to NoUart
    state_publisher_1.publish_immediate(MeetingSignState::NoUart);

    let state_subscriber_1 = meeting_sign_state
        .subscriber()
        .expect("Failed to create state subscriber 1");
    let state_subscriber_2 = meeting_sign_state
        .subscriber()
        .expect("Failed to create state subscriber 2");
    let mut state_subscriber_3 = meeting_sign_state
        .subscriber()
        .expect("Failed to create state subscriber 3");

    let rx_pin = peripherals.GPIO1;
    let uart_config = Config::default().with_rx(
        RxConfig::default().with_fifo_full_threshold(meeting_instruction::READ_BUF_SIZE as u16),
    );
    let uart = Uart::new(peripherals.UART0, uart_config)
        .unwrap()
        .with_rx(rx_pin)
        .into_async();

    spawner
        .spawn(uart_reader(uart, state_publisher_1, state_subscriber_1))
        .ok();
    spawner
        .spawn(uart_timeout_monitor(state_publisher_2, state_subscriber_2))
        .ok();

    // Initialize hardcoded timer if no UART within threshold
    match select(
        state_subscriber_3.next_message_pure(),
        Timer::after(UART_COMMUNICATION_TIMEOUT),
    )
    .await
    {
        Either::First(state) => {
            match state {
                MeetingSignState::NoUart => {
                    info!("State changed to NoUart.");
                    // Turn off all LEDs
                    leds.lock().await.set_portion_high(1.0, 2.0);
                }
                MeetingSignState::Uart => {
                    info!("State changed to Uart.");
                    // Set LEDs to indicate UART communication
                    leds.lock().await.set_portion_high(1.0, 1.0);
                }
            }
        }
        Either::Second(_) => {
            info!("No initial state change detected within {}s, initializing LEDs according to hardcoded timer...", 
                UART_COMMUNICATION_TIMEOUT.as_secs());
        }
    }

    loop {
        match select(
            state_subscriber_3.next_message_pure(),
            Timer::after_secs(60),
        )
        .await
        {
            Either::First(state) => {
                match state {
                    MeetingSignState::NoUart => {
                        info!("State changed to NoUart.");
                        // Turn off all LEDs
                        leds.lock().await.set_portion_high(1.0, 2.0);
                    }
                    MeetingSignState::Uart => {
                        info!("State changed to Uart.");
                        // Set LEDs to indicate UART communication
                        leds.lock().await.set_portion_high(1.0, 1.0);
                    }
                }
            }
            Either::Second(_) => {
                info!("No state change detected within 60 seconds, updating LEDs according to internal timer...");
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

    /// Set the portion of LEDs to high based on the given numerator and denominator.
    pub fn set_portion_high(&mut self, numerator: f32, denominator: f32) {
        let num_on_leds = if numerator <= 0.0 || denominator <= 0.0 {
            warn!(
                "Invalid portion values: numerator {}, denominator {}",
                numerator, denominator
            );
            0 // This will turn off all LEDs
        } else {
            (NUM_LEDS as f32 * numerator / denominator).round() as usize
        };

        for (led_idx, led) in self.led_outs.iter_mut().enumerate() {
            if led_idx < num_on_leds {
                led.set_high();
            } else {
                led.set_low();
            }
        }
    }
}

#[embassy_executor::task]
async fn uart_reader(
    mut uart: Uart<'static, Async>,
    state_publisher: MeetingSignStatePublisher<'static>,
    mut state_subscriber: MeetingSignStateSubscriber<'static>,
) {
    debug!("Starting UART reader task");

    // Buffers sized appropriately for the MeetingInstruction payload
    let mut read_buf = [0u8; meeting_instruction::RX_BUFFER_SIZE];
    let mut decode_buf = [0u8; meeting_instruction::MAX_ENCODED_SIZE];

    let mut offset = 0;
    loop {
        trace!("Beginning read loop, current offset: {}", offset);
        match embedded_io_async::Read::read(&mut uart, &mut read_buf[offset..]).await {
            Ok(len) => {
                offset += len;
                trace!("Read {} bytes, total buffer: {}", len, offset);

                // Look for delimiter
                if let Some(delimiter_pos) = read_buf[..offset].iter().position(|&b| b == 0x00) {
                    let encoded_data = &read_buf[..delimiter_pos];
                    trace!("Received encoded data: {:?}", encoded_data);

                    match cobs::decode(encoded_data, &mut decode_buf) {
                        Ok(decoded_len) => {
                            let decoded_data = &decode_buf[..decoded_len];
                            trace!("Decoded data: {:?}", decoded_data);
                            match postcard::from_bytes::<MeetingSignInstruction>(decoded_data) {
                                Ok(instruction) => {
                                    debug!("Received: {:?}", instruction);
                                    match state_subscriber.try_next_message_pure() {
                                        None | Some(MeetingSignState::NoUart) => {
                                            // If we were not in UART state, switch to UART
                                            debug!("Switching state to UART");
                                            state_publisher
                                                .publish_immediate(MeetingSignState::Uart);
                                        }
                                        Some(MeetingSignState::Uart) => {
                                            // Already in UART state, just log
                                            debug!("Already in UART state, processing instruction");
                                        }
                                    }
                                }
                                Err(e) => warn!("Deserialization error: {:?}", e),
                            }
                        }
                        Err(e) => {
                            warn!("COBS decode error: {:?}", e);
                        }
                    }
                    offset = 0; // Reset after processing
                }
            }
            Err(e) => error!("UART read error: {:?}", e),
        }

        // Prevent buffer overflow
        if offset >= meeting_instruction::RX_BUFFER_SIZE - 1 {
            warn!("Buffer overflow, resetting");
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
                MeetingSignState::Uart => break,      // Start monitoring
                MeetingSignState::NoUart => continue, // Keep waiting
            }
        }

        // Now we're in UART state, start the timeout monitoring
        let mut timeout_timer = Timer::after(UART_COMMUNICATION_TIMEOUT);

        loop {
            match select(state_subscriber.next_message_pure(), &mut timeout_timer).await {
                Either::First(state) => {
                    match state {
                        MeetingSignState::Uart => {
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
        // Try to set the panic pin high
        unsafe {
            if let Some(ref mut pin) = PANIC_PIN {
                pin.set_low();
            } else {
                emergency_gpio3_low();
            }

            set_leds_panic_pattern();
        }
    });

    error!("Panic occurred: {:?}", info);

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

/// Emergency function to set GPIO pin low using direct register access
unsafe fn emergency_gpio3_low() {
    const GPIO_PIN_NUMBER: u32 = 3;

    // Enable GPIO pin as output
    let enable_val = core::ptr::read_volatile(GPIO_ENABLE_REG);
    core::ptr::write_volatile(GPIO_ENABLE_REG, enable_val | (1 << GPIO_PIN_NUMBER));

    // Set GPIO pin low (clear the bit)
    let out_val = core::ptr::read_volatile(GPIO_OUT_REG);
    core::ptr::write_volatile(GPIO_OUT_REG, out_val & !(1 << GPIO_PIN_NUMBER));
}

/// Panic function to set panic pattern on LEDs via GPIO pins
unsafe fn set_leds_panic_pattern() {
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
