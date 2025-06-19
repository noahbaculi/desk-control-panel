#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use desk_control_panel::meeting_instruction::{self, MeetingSignInstruction};
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
// use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    gpio::{Input, InputConfig, Level, Output, OutputConfig},
    timer::systimer::SystemTimer,
    uart::{Config, RxConfig, Uart},
    Async,
};
use log::{debug, error, info, trace, warn, LevelFilter};

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

// Global static for the panic pin
static mut PANIC_PIN: Option<Output<'static>> = None;

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

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    esp_println::logger::init_logger(LevelFilter::Debug);

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    info!("Embassy initialized!");
    let rx_pin = peripherals.GPIO1;

    let config = Config::default().with_rx(
        RxConfig::default().with_fifo_full_threshold(meeting_instruction::READ_BUF_SIZE as u16),
    );

    let uart = Uart::new(peripherals.UART0, config)
        .unwrap()
        .with_rx(rx_pin)
        .into_async();

    spawner.spawn(uart_reader(uart)).ok();

    // WARN: This panic pin needs to match the emergency hardcoded panic pin
    let panic_pin = Output::new(peripherals.GPIO3, Level::High, OutputConfig::default());
    // Store the pin globally for panic handler access
    unsafe {
        PANIC_PIN = Some(panic_pin);
    }

    Timer::after(Duration::from_secs(5)).await;
    panic!("Ahhhh");
}

#[panic_handler]
fn panic_handler(_info: &core::panic::PanicInfo) -> ! {
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
        }
    });

    error!("Panic occurred: {:?}", _info);

    // Halt the system
    loop {
        core::hint::spin_loop();
    }
}

/// Emergency function to set GPIO pin low using direct register access
unsafe fn emergency_gpio3_low() {
    // ESP32-C3 GPIO register addresses (from ESP32-C3 TRM Table 3.3-3 and Section 5.14.1)
    // GPIO base address: 0x60004000, offsets from Section 5.14.1:
    // https://www.espressif.com/sites/default/files/documentation/esp32-c3_technical_reference_manual_en.pdf

    const GPIO_BASE: u32 = 0x60004000;
    const GPIO_OUT_REG: *mut u32 = (GPIO_BASE + 0x0004) as *mut u32; // GPIO output register
    const GPIO_ENABLE_REG: *mut u32 = (GPIO_BASE + 0x0020) as *mut u32; // GPIO output enable register

    const GPIO_PIN_NUMBER: u32 = 3;

    // Enable GPIO pin as output
    let enable_val = core::ptr::read_volatile(GPIO_ENABLE_REG);
    core::ptr::write_volatile(GPIO_ENABLE_REG, enable_val | (1 << GPIO_PIN_NUMBER));

    // Set GPIO pin low (clear the bit)
    let out_val = core::ptr::read_volatile(GPIO_OUT_REG);
    core::ptr::write_volatile(GPIO_OUT_REG, out_val & !(1 << GPIO_PIN_NUMBER));
}

// Function you can call manually in error conditions
pub fn trigger_panic_signal() {
    unsafe {
        emergency_gpio3_low();
    }
}
