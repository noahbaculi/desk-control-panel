#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use desk_control_panel::meeting_instruction::{
    MeetingSignInstruction, MAX_ENCODED_SIZE, READ_BUF_SIZE, RX_BUFFER_SIZE,
};
use embassy_executor::Spawner;
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    timer::systimer::SystemTimer,
    uart::{Config, RxConfig, Uart},
    Async,
};
use log::{error, info, LevelFilter};

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[embassy_executor::task]
async fn reader(mut uart: Uart<'static, Async>) {
    info!("Hi!");
    let mut rbuf = [0u8; RX_BUFFER_SIZE];
    let mut decode_buf = [0u8; MAX_ENCODED_SIZE];
    let mut offset = 0;

    loop {
        info!("Start loop");
        match embedded_io_async::Read::read(&mut uart, &mut rbuf[offset..offset + 1]).await {
            Ok(len) => {
                offset += len;
            }
            Err(e) => {
                error!("UART read error: {:?}", e);
            }
        }

        // Check for null delimiter
        if rbuf[offset - 1] == 0x00 {
            let encoded_data = &rbuf[..offset - 1]; // Exclude delimiter
            info!("Received encoded data: {:?}", encoded_data);

            // COBS decode
            match cobs::decode(encoded_data, &mut decode_buf) {
                Ok(decoded_len) => {
                    let decoded_data = &decode_buf[..decoded_len];
                    info!("Decoded data: {:?}", decoded_data);

                    // Deserialize
                    match postcard::from_bytes::<MeetingSignInstruction>(decoded_data) {
                        Ok(instruction) => {
                            info!("Received: {:?}", instruction);
                        }
                        Err(e) => {
                            info!("Deserialization error: {:?}", e);
                        }
                    }
                }
                Err(e) => {
                    info!("COBS decode error: {:?}", e);
                    info!("Raw encoded data: {:?}", encoded_data);
                }
            }
            offset = 0; // Reset for next message
        }

        // Prevent buffer overflow
        if offset >= RX_BUFFER_SIZE - 1 {
            info!("Buffer overflow, resetting");
            offset = 0;
        }
    }
}

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    esp_println::logger::init_logger(LevelFilter::Info);

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    info!("Embassy initialized!");

    let rx_pin = peripherals.GPIO21;

    let config = Config::default()
        .with_rx(RxConfig::default().with_fifo_full_threshold(READ_BUF_SIZE as u16));

    let uart = Uart::new(peripherals.UART0, config)
        .unwrap()
        .with_rx(rx_pin)
        .into_async();

    spawner.spawn(reader(uart)).ok();
}
