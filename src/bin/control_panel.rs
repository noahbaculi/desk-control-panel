#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use defmt::info;
use desk_control_panel::{
    MeetingSignInstruction, AT_CMD, MAX_ENCODED_SIZE, MAX_PAYLOAD_SIZE, READ_BUF_SIZE,
};
use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    timer::systimer::SystemTimer,
    uart::{AtCmdConfig, Config, RxConfig, Uart, UartTx},
    Async,
};
use static_cell::StaticCell;
use {esp_backtrace as _, esp_println as _};

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[embassy_executor::task]
async fn writer(mut tx: UartTx<'static, Async>, _signal: &'static Signal<NoopRawMutex, usize>) {
    let payload = MeetingSignInstruction::Duration(180);

    // Buffers sized appropriately for COBS
    let mut serialize_buf = [0u8; MAX_PAYLOAD_SIZE];
    let mut encode_buf = [0u8; MAX_ENCODED_SIZE];

    loop {
        // Serialize the payload
        let serialized = postcard::to_slice(&payload, &mut serialize_buf).unwrap();
        let serialized_len = serialized.len();

        // COBS encode
        let encoded_len = cobs::encode(serialized, &mut encode_buf);

        esp_println::println!(
            "Serialized: {} bytes, Encoded: {} bytes",
            serialized_len,
            encoded_len
        );
        esp_println::println!("Raw data: {:?}", &serialized);
        esp_println::println!("Encoded data: {:?}", &encode_buf[..encoded_len]);

        // Send encoded data + null delimiter
        tx.write_async(&encode_buf[..encoded_len]).await.unwrap();
        tx.write_async(&[0x00]).await.unwrap();
        embedded_io_async::Write::flush(&mut tx).await.unwrap();

        Timer::after(Duration::from_millis(5000)).await;
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

    let tx_pin = peripherals.GPIO20;

    let config = Config::default()
        .with_rx(RxConfig::default().with_fifo_full_threshold(READ_BUF_SIZE as u16));

    let mut uart0 = Uart::new(peripherals.UART0, config)
        .unwrap()
        .with_tx(tx_pin)
        // .with_rx(rx_pin)
        .into_async();
    uart0.set_at_cmd(AtCmdConfig::default().with_cmd_char(AT_CMD));

    let (_rx, tx) = uart0.split();

    static SIGNAL: StaticCell<Signal<NoopRawMutex, usize>> = StaticCell::new();
    let signal = &*SIGNAL.init(Signal::new());

    // spawner.spawn(reader(rx, signal)).ok();
    spawner.spawn(writer(tx, signal)).ok();
}
