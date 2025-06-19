#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use desk_control_panel::meeting_instruction::{self, MeetingSignInstruction};
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer};
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

type LEDsMutex = Mutex<CriticalSectionRawMutex, LEDs<'static>>;
static LEDS: StaticCell<LEDsMutex> = StaticCell::new();

struct LEDs<'a> {
    led_outs: [Output<'a>; 9],
}
impl<'a> LEDs<'a> {
    pub fn new(pins: [AnyPin<'a>; 9]) -> Self {
        // Convert each pin to an Output
        let led_outs = pins.map(|pin| Output::new(pin, Level::Low, OutputConfig::default()));

        Self { led_outs }
    }

    pub fn toggle_all(&mut self) {
        for led in &mut self.led_outs {
            led.toggle();
        }
    }

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
            // if led_idx + 1 <= num_on_leds {
            if led_idx < num_on_leds {
                led.set_high();
            } else {
                led.set_low();
            }
        }
    }
}

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
    let led_pins: [AnyPin; 9] = [
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
    spawner.spawn(led_test_task(leds)).ok();

    let rx_pin = peripherals.GPIO1;
    let uart_config = Config::default().with_rx(
        RxConfig::default().with_fifo_full_threshold(meeting_instruction::READ_BUF_SIZE as u16),
    );
    let uart = Uart::new(peripherals.UART0, uart_config)
        .unwrap()
        .with_rx(rx_pin)
        .into_async();

    spawner.spawn(uart_reader(uart)).ok();

    Timer::after(Duration::from_secs(5)).await;
    panic!("Ahhhh");
}

#[embassy_executor::task]
async fn led_test_task(leds: &'static LEDsMutex) {
    let mut counter = 0u16;
    loop {
        {
            let mut leds = leds.lock().await;
            leds.toggle_all();
        }
        counter = counter.wrapping_add(1);
        Timer::after(Duration::from_millis(500)).await;
    }
}

#[embassy_executor::task]
async fn uart_reader(mut uart: Uart<'static, Async>) {
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

#[panic_handler]
fn panic_handler(info: &core::panic::PanicInfo) -> ! {
    // Disable interrupts to prevent further issues
    critical_section::with(|_| {
        // Try to set the panic pin high
        unsafe {
            if let Some(ref mut pin) = PANIC_PIN {
                pin.set_low();
            } else {
                // Fallback: directly access GPIO registers if pin wasn't initialized
                // This is a last resort and uses unsafe register access
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
