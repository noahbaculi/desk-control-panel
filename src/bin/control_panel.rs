#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use desk_control_panel::meeting_duration::MeetingDuration;
use desk_control_panel::meeting_instruction::{
    MeetingSignInstruction, MAX_ENCODED_SIZE, MAX_PAYLOAD_SIZE, READ_BUF_SIZE,
};
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::timer::systimer::SystemTimer;
use esp_hal::{
    uart::{Config, RxConfig, Uart},
    Async,
};
use log::info;
use log::LevelFilter;

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[embassy_executor::task]
async fn writer(mut uart: Uart<'static, Async>) {
    // Buffers sized appropriately for COBS
    let mut serialize_buf = [0u8; MAX_PAYLOAD_SIZE];
    let mut encode_buf = [0u8; MAX_ENCODED_SIZE];

    loop {
        for num_minutes in 1..=180 {
            let duration =
                MeetingDuration::from_minutes(num_minutes).expect("Could not create duration");
            let payload: MeetingSignInstruction = duration.into();

            // Serialize the payload
            let serialized = postcard::to_slice(&payload, &mut serialize_buf).unwrap();
            let serialized_len = serialized.len();

            // COBS encode
            let encoded_len = cobs::encode(serialized, &mut encode_buf);

            info!("{:?}", &duration);
            info!(
                "Serialized: {} bytes, Encoded: {} bytes",
                serialized_len, encoded_len
            );
            info!(
                "Minutes: {:?} | Instruction: {:?} | Raw data: {:?} | Encoded data: {:?}",
                num_minutes,
                payload,
                &serialized,
                &encode_buf[..encoded_len]
            );

            // Send encoded data + null delimiter
            uart.write_async(&encode_buf[..encoded_len]).await.unwrap();
            uart.write_async(&[0x00]).await.unwrap();
            embedded_io_async::Write::flush(&mut uart).await.unwrap();

            Timer::after(Duration::from_millis(100)).await;
        }
    }
}

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    esp_println::logger::init_logger(LevelFilter::Info);

    info!("Hi there!");

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    info!("Embassy initialized!");

    let tx_pin = peripherals.GPIO21;

    let config = Config::default()
        .with_rx(RxConfig::default().with_fifo_full_threshold(READ_BUF_SIZE as u16));

    let uart = Uart::new(peripherals.UART0, config)
        .unwrap()
        .with_tx(tx_pin)
        .into_async();

    spawner.spawn(writer(uart)).ok();
}
