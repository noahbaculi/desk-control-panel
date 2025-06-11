#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use defmt::info;
use desk_control_panel::{
    MeetingSignInstruction, AT_CMD, MAX_PAYLOAD_SIZE, READ_BUF_SIZE, RX_BUFFER_SIZE,
};
use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    timer::systimer::SystemTimer,
    uart::{AtCmdConfig, Config, RxConfig, Uart, UartRx},
    Async,
};
use static_cell::StaticCell;
use {esp_backtrace as _, esp_println as _};

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[embassy_executor::task]
async fn reader(mut rx: UartRx<'static, Async>, _signal: &'static Signal<NoopRawMutex, usize>) {
    let mut rbuf: [u8; RX_BUFFER_SIZE] = [0u8; RX_BUFFER_SIZE];
    let mut decode_buf = [0u8; MAX_PAYLOAD_SIZE];
    let mut offset = 0;

    loop {
        // Read one byte at a time
        let len = embedded_io_async::Read::read(&mut rx, &mut rbuf[offset..offset + 1])
            .await
            .unwrap();
        offset += len;

        // Check for null delimiter
        if rbuf[offset - 1] == 0x00 {
            let encoded_data = &rbuf[..offset - 1]; // Exclude delimiter
            esp_println::println!("Received encoded data: {:?}", encoded_data);

            // COBS decode
            match cobs::decode(encoded_data, &mut decode_buf) {
                Ok(decoded_len) => {
                    let decoded_data = &decode_buf[..decoded_len];
                    esp_println::println!("Decoded data: {:?}", decoded_data);

                    // Deserialize
                    match postcard::from_bytes::<MeetingSignInstruction>(decoded_data) {
                        Ok(instruction) => {
                            esp_println::println!("✅ Received: {:?}", instruction);
                        }
                        Err(e) => {
                            esp_println::println!("❌ Deserialization error: {:?}", e);
                        }
                    }
                }
                Err(e) => {
                    esp_println::println!("❌ COBS decode error: {:?}", e);
                    esp_println::println!("Raw encoded data: {:?}", encoded_data);
                }
            }

            offset = 0; // Reset for next message
        }

        // Prevent buffer overflow
        if offset >= RX_BUFFER_SIZE - 1 {
            esp_println::println!("⚠️  Buffer overflow, resetting");
            offset = 0;
        }
    }
}
#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    // generator version: 0.4.0

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    info!("Embassy initialized!");

    let rx_pin = peripherals.GPIO20;

    let config = Config::default()
        .with_rx(RxConfig::default().with_fifo_full_threshold(READ_BUF_SIZE as u16));

    let mut uart0 = Uart::new(peripherals.UART0, config)
        .unwrap()
        // .with_tx(tx_pin)
        .with_rx(rx_pin)
        .into_async();
    uart0.set_at_cmd(AtCmdConfig::default().with_cmd_char(AT_CMD));

    let (rx, _tx) = uart0.split();

    static SIGNAL: StaticCell<Signal<NoopRawMutex, usize>> = StaticCell::new();
    let signal = &*SIGNAL.init(Signal::new());

    spawner.spawn(reader(rx, signal)).ok();
    // spawner.spawn(writer(tx, signal)).ok();
}
