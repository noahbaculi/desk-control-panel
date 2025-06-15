#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use desk_control_panel::meeting_instruction::{self, MeetingSignInstruction};
use embassy_executor::Spawner;
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    timer::systimer::SystemTimer,
    uart::{Config, RxConfig, Uart},
    Async,
};
use log::{debug, error, info, trace, warn, LevelFilter};

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

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

    let rx_pin = peripherals.GPIO21;

    let config = Config::default().with_rx(
        RxConfig::default().with_fifo_full_threshold(meeting_instruction::READ_BUF_SIZE as u16),
    );

    let uart = Uart::new(peripherals.UART0, config)
        .unwrap()
        .with_rx(rx_pin)
        .into_async();

    spawner.spawn(uart_reader(uart)).ok();
}
