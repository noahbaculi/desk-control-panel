#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![allow(unused_imports)]

use desk_control_panel::control_panel::state::{
    ControlPanelState, MeetingSignState, MovementDirection, UISection, UISelectionMode,
    USBPowerState, USBSwitchOutput, USBSwitchState,
};
use desk_control_panel::meeting_duration::MeetingDuration;
use desk_control_panel::meeting_instruction::{self, MeetingSignInstruction};
use embassy_executor::Spawner;
use embassy_futures::select::select;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Ticker, Timer};
use embedded_graphics::primitives::{Line, Polyline};
use embedded_graphics::{
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
};
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull};
use esp_hal::i2c::master::I2c;
use esp_hal::time::Rate;
use esp_hal::timer::systimer::SystemTimer;
use esp_hal::{
    uart::{Config, RxConfig, Uart},
    Async,
};
use log::LevelFilter;
use log::{debug, info};
use rotary_encoder_hal::{Direction, Rotary};
use ssd1306::mode::DisplayConfig;
use ssd1306::{I2CDisplayInterface, Ssd1306};
use static_cell::StaticCell;

extern crate alloc;

// type StateMutex = Mutex<CriticalSectionRawMutex, ControlPanelState<DrawTarget<BinaryColor>>>;
type StateMutex = Mutex<CriticalSectionRawMutex, ControlPanelState>;
static STATE_MUTEX: StaticCell<StateMutex> = StaticCell::new();
// static STATE_MUTEX: StateMutex = StateMutex::new(ControlPanelState {
//     usb_switch: USBSwitchState::Off,
//     meeting_sign: MeetingSignState::Off,
//     usb_power_1: USBPowerState::Off,
//     usb_power_2: USBPowerState::Off,
//     ui_selection_mode: RotaryEncoderSelectionMode::Menu,
//     ui_section: UISection::MeetingSign,
// });

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    // esp_println::logger::init_logger(LevelFilter::Debug);
    esp_println::logger::init_logger(LevelFilter::Info);

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    info!("Embassy initialized!");

    let usb_power_1 = Output::new(peripherals.GPIO9, Level::Low, OutputConfig::default());
    let usb_power_2 = Output::new(peripherals.GPIO10, Level::Low, OutputConfig::default());

    let meeting_sign_power = Output::new(peripherals.GPIO5, Level::Low, OutputConfig::default());
    // This signal should be 3.3V high when the Meeting Sign is operating correctly
    let meeting_sign_sense = Input::new(
        peripherals.GPIO6,
        InputConfig::default().with_pull(Pull::Down),
    );

    // Initialize and configure I2C
    let i2c = I2c::new(
        peripherals.I2C0,
        esp_hal::i2c::master::Config::default().with_frequency(Rate::from_khz(400)),
    )
    .unwrap()
    .with_sda(peripherals.GPIO0)
    .with_scl(peripherals.GPIO1);

    let interface = I2CDisplayInterface::new(i2c);
    let mut display = Ssd1306::new(
        interface,
        ssd1306::size::DisplaySize128x64,
        ssd1306::prelude::DisplayRotation::Rotate0,
    )
    .into_buffered_graphics_mode();
    display.init().unwrap();

    // Clear the display once at startup
    display.clear(BinaryColor::Off).unwrap();
    display.flush().unwrap();

    let control_panel_state = STATE_MUTEX.init(StateMutex::new(ControlPanelState {
        usb_switch_state: USBSwitchState::Off,
        usb_power_1,
        usb_power_2,
        meeting_sign_power,
        ui_selection_mode: UISelectionMode::Menu,
        ui_section: UISection::MeetingSign,
        display,
    }));
    control_panel_state.lock().await.draw_ui().unwrap();

    spawner
        .spawn(monitor_meeting_sign_sense(
            meeting_sign_sense,
            control_panel_state,
        ))
        .ok();

    let usb_switch_led_a = Input::new(
        peripherals.GPIO20,
        InputConfig::default().with_pull(Pull::Down),
    );
    let usb_switch_led_b = Input::new(
        peripherals.GPIO21,
        InputConfig::default().with_pull(Pull::Down),
    );
    spawner
        .spawn(monitor_usb_switch_leds(
            usb_switch_led_a,
            usb_switch_led_b,
            control_panel_state,
        ))
        .ok();

    let meeting_sign_uart_pin = peripherals.GPIO7;
    let meeting_sign_uart_config = Config::default().with_rx(
        RxConfig::default().with_fifo_full_threshold(meeting_instruction::READ_BUF_SIZE as u16),
    );
    let meeting_sign_uart = Uart::new(peripherals.UART0, meeting_sign_uart_config)
        .unwrap()
        .with_tx(meeting_sign_uart_pin)
        .into_async();
    spawner.spawn(writer(meeting_sign_uart)).ok();

    let rotary_encoder_button = Input::new(peripherals.GPIO2, InputConfig::default());
    info!(
        "Rotary encoder button is {:?}!",
        rotary_encoder_button.level()
    );
    spawner
        .spawn(monitor_rotary_encoder_button(
            rotary_encoder_button,
            control_panel_state,
        ))
        .ok();

    let rotary_encoder_clk = Input::new(
        peripherals.GPIO4,
        InputConfig::default().with_pull(Pull::Up),
    );
    let rotary_encoder_dt = Input::new(
        peripherals.GPIO3,
        InputConfig::default().with_pull(Pull::Up),
    );
    spawner
        .spawn(monitor_rotary_encoder_rotation(
            rotary_encoder_clk,
            rotary_encoder_dt,
            control_panel_state,
        ))
        .ok();

    // let mut ticker = Ticker::every(Duration::from_millis(100));
    //
    // // Main loop
    // loop {
    //     {
    //         // Draw USB switch state
    //         control_panel_state
    //             .lock()
    //             .await
    //             .usb_switch
    //             .draw(&mut display)
    //             .unwrap();
    //     }
    //     {
    //         control_panel_state
    //             .lock()
    //             .await
    //             .draw_ui(&mut display)
    //             .unwrap();
    //     }
    //
    //     display.flush().unwrap();
    //     ticker.next().await;
    // }
}

#[embassy_executor::task]
async fn monitor_rotary_encoder_rotation(
    rotary_encoder_clk: Input<'static>,
    rotary_encoder_dt: Input<'static>,
    control_panel_state: &'static StateMutex,
) {
    debug!("Starting monitor_rotary_encoder_rotation task");
    let mut rotary_encoder = Rotary::new(rotary_encoder_dt, rotary_encoder_clk);
    let mut counter = 0;
    loop {
        let direction = rotary_encoder.update().unwrap();
        match direction {
            Direction::Clockwise => {
                counter += 1;
                info!("Rotary encoder {:?}! Counter = {}", direction, counter);
                control_panel_state
                    .lock()
                    .await
                    .rotary_encoder_rotate(MovementDirection::Clockwise);
            }
            Direction::CounterClockwise => {
                counter -= 1;
                info!("Rotary encoder {:?}! Counter = {}", direction, counter);
                control_panel_state
                    .lock()
                    .await
                    .rotary_encoder_rotate(MovementDirection::CounterClockwise);
            }
            Direction::None => {}
        }

        Timer::after(Duration::from_millis(3)).await;
    }
}

#[embassy_executor::task]
async fn monitor_rotary_encoder_button(
    mut button: Input<'static>,
    control_panel_state: &'static StateMutex,
) {
    debug!("Starting monitor_rotary_encoder_button task");
    let mut counter = 0;
    loop {
        button.wait_for_falling_edge().await;
        counter += 1;
        info!("Rotary encoder button pressed! Counter = {}", counter);

        {
            let mut control_panel = control_panel_state.lock().await;
            control_panel.ui_selection_mode.toggle();
            control_panel.draw_border_ui().unwrap();
            control_panel.display.flush().unwrap();
        }

        // Debounce the button press
        Timer::after(Duration::from_millis(200)).await;
    }
}

#[embassy_executor::task]
async fn monitor_meeting_sign_sense(
    mut digital_input: Input<'static>,
    control_panel_state: &'static StateMutex,
) {
    debug!("Starting monitor_meeting_sign_sense task");
    loop {
        digital_input.wait_for_any_edge().await;
        // Debounce the change
        Timer::after(Duration::from_millis(100)).await;

        info!("Meeting Sign sense changed to {:?}", digital_input.level());

        // control_panel_state
        //     .lock()
        //     .await
        //     .draw_usb_switch_ui()
        //     .unwrap();
    }
}

#[embassy_executor::task]
async fn monitor_usb_switch_leds(
    mut led_a: Input<'static>,
    mut led_b: Input<'static>,
    control_panel_state: &'static StateMutex,
) {
    debug!("Starting monitor_usb_switch_leds task");
    loop {
        select(led_a.wait_for_any_edge(), led_b.wait_for_any_edge()).await;

        // Debounce the change
        Timer::after(Duration::from_millis(100)).await;

        let usb_switch_state = match (led_a.level(), led_b.level()) {
            (Level::Low, Level::Low) | (Level::High, Level::High) => USBSwitchState::Off,
            (Level::High, Level::Low) => USBSwitchState::On(USBSwitchOutput::A),
            (Level::Low, Level::High) => USBSwitchState::On(USBSwitchOutput::B),
        };
        info!("USB Switch leds sense changed to {:?}", &usb_switch_state);

        control_panel_state
            .lock()
            .await
            .update_usb_switch_state(usb_switch_state)
            .unwrap();
    }
}

#[embassy_executor::task]
async fn writer(mut uart: Uart<'static, Async>) {
    debug!("Starting UART writer task");

    // Buffers sized appropriately for the MeetingInstruction payload
    let mut serialize_buf = [0u8; meeting_instruction::MAX_PAYLOAD_SIZE];
    let mut encode_buf = [0u8; meeting_instruction::MAX_ENCODED_SIZE];

    loop {
        for num_minutes in 1..=120 {
            let duration =
                MeetingDuration::from_minutes(num_minutes).expect("Could not create duration");
            let payload = MeetingSignInstruction::On(
                meeting_instruction::ProgressRatio::from_durations(
                    &duration.into(),
                    &MeetingDuration::MAX.into(),
                )
                .expect("Invalid progress ratio"),
            );

            // Serialize the payload
            let serialized = postcard::to_slice(&payload, &mut serialize_buf).unwrap();
            let serialized_len = serialized.len();

            // COBS encode
            let encoded_len = cobs::encode(serialized, &mut encode_buf);

            debug!("{:?}", &duration);
            debug!(
                "Serialized: {} bytes, Encoded: {} bytes",
                serialized_len, encoded_len
            );
            debug!(
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
