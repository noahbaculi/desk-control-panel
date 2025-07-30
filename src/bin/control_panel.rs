#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use desk_control_panel::control_panel::state::{
    ControlPanelState, MeetingSignState, MovementDirection, UISection, UISelectionMode,
    USBSwitchState,
};
use desk_control_panel::meeting_duration::MeetingDuration;
use desk_control_panel::meeting_instruction::{
    self, MeetingSignInstruction, COBS_DELIMITER, UART_COMMUNICATION_INTERVAL,
};
use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant, Ticker, Timer};
use embedded_graphics::{pixelcolor::BinaryColor, prelude::*};
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{DriveMode, Input, InputConfig, Level, Output, OutputConfig, Pull};
use esp_hal::i2c::master::I2c;
use esp_hal::peripherals::LPWR;
use esp_hal::rtc_cntl::sleep::{GpioWakeupSource, RtcioWakeupSource, WakeupLevel};
use esp_hal::rtc_cntl::Rtc;
use esp_hal::time::Rate;
use esp_hal::timer::systimer::SystemTimer;
use esp_hal::{
    uart::{Config, RxConfig, Uart},
    Async,
};
use log::{debug, info};
use log::{error, LevelFilter};
use rotary_encoder_hal::{Direction, Rotary};
use ssd1306::mode::DisplayConfig;
use ssd1306::{I2CDisplayInterface, Ssd1306};
use static_cell::StaticCell;

type StateMutex = Mutex<CriticalSectionRawMutex, ControlPanelState>;
static STATE_MUTEX: StaticCell<StateMutex> = StaticCell::new();

// This signal is used to efficiently monitor the state of the Meeting Sign timer only when the Meeting Sign is active.
static MEETING_SIGN_STATE: Signal<CriticalSectionRawMutex, MeetingSignState> = Signal::new();

// This signal is used to delay the sleep timer task when inputs are received.
static SLEEP_TIMER_EXTENSION_SIGNAL: Signal<CriticalSectionRawMutex, ()> = Signal::new();

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    // esp_println::logger::init_logger(LevelFilter::Debug);
    esp_println::logger::init_logger(LevelFilter::Info);

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let mut peripherals = esp_hal::init(config);

    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    info!("Embassy initialized!");

    // NOTE: Since this is a P-Channel MOSFET, the MOSFET is "off" when the gate is high/floating.
    let usb_power_1 = Output::new(
        peripherals.GPIO9,
        Level::High,
        OutputConfig::default().with_drive_mode(DriveMode::OpenDrain),
    );
    let usb_power_2 = Output::new(
        peripherals.GPIO10,
        Level::High,
        OutputConfig::default().with_drive_mode(DriveMode::OpenDrain),
    );

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

    let usb_switch_led_a = Input::new(
        peripherals.GPIO20,
        InputConfig::default().with_pull(Pull::Down),
    );
    let usb_switch_led_b = Input::new(
        peripherals.GPIO21,
        InputConfig::default().with_pull(Pull::Down),
    );
    let usb_switch_state = USBSwitchState::from_leds(&usb_switch_led_a, &usb_switch_led_b);

    let control_panel_state = STATE_MUTEX.init(StateMutex::new(ControlPanelState {
        usb_switch_state,
        usb_power_1,
        usb_power_2,
        meeting_sign_sense,
        meeting_sign_power,
        ui_selection_mode: UISelectionMode::Menu,
        ui_section: UISection::MeetingSign,
        display,
        meeting_sign_completion: None,
    }));
    {
        let mut cps = control_panel_state.lock().await;
        cps.draw_entire_ui().unwrap();
        cps.display.flush().unwrap();
    }

    spawner
        .spawn(monitor_usb_switch_leds(
            usb_switch_led_a,
            usb_switch_led_b,
            control_panel_state,
        ))
        .ok();

    let meeting_sign_uart_pin = peripherals.GPIO7;
    let meeting_sign_uart_config = Config::default().with_rx(
        RxConfig::default().with_fifo_full_threshold(meeting_instruction::FIFO_THRESHOLD as u16),
    );
    let meeting_sign_uart = Uart::new(peripherals.UART0, meeting_sign_uart_config)
        .unwrap()
        .with_tx(meeting_sign_uart_pin)
        .into_async();
    spawner
        .spawn(writer(meeting_sign_uart, control_panel_state))
        .ok();

    let mut gpio2 = peripherals.GPIO2;
    let rotary_encoder_button = Input::new(gpio2, InputConfig::default());
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

    spawner
        .spawn(monitor_meeting_sign_timer(control_panel_state))
        .ok();

    let wakeup_pins: &mut [(&mut dyn esp_hal::gpio::RtcPinWithResistors, WakeupLevel)] = &mut [
        // (&mut gpio2, WakeupLevel::High),
    ];
    let rtcio_wakeup_source = RtcioWakeupSource::new(wakeup_pins);

    let low_power_peripheral = peripherals.LPWR;
    spawner.must_spawn(sleep_timer(
        low_power_peripheral,
        rtcio_wakeup_source,
        control_panel_state,
    ));
}

#[embassy_executor::task]
async fn monitor_rotary_encoder_rotation(
    rotary_encoder_clk: Input<'static>,
    rotary_encoder_dt: Input<'static>,
    control_panel_state: &'static StateMutex,
) {
    debug!("Starting monitor_rotary_encoder_rotation task");
    let mut rotary_encoder = Rotary::new(rotary_encoder_dt, rotary_encoder_clk);
    loop {
        let direction = rotary_encoder.update().unwrap();
        match direction {
            Direction::Clockwise => {
                let mut cps = control_panel_state.lock().await;
                cps.rotary_encoder_rotate(&MEETING_SIGN_STATE, MovementDirection::Clockwise);
                cps.display.flush().unwrap();
            }
            Direction::CounterClockwise => {
                let mut cps = control_panel_state.lock().await;
                cps.rotary_encoder_rotate(&MEETING_SIGN_STATE, MovementDirection::CounterClockwise);
                cps.display.flush().unwrap();
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
        info!("Rotary encoder button pressed! Counter = {counter}");

        {
            let mut cps = control_panel_state.lock().await;
            cps.rotary_encoder_press();
            cps.display.flush().unwrap();
        }

        // Debounce the button press
        Timer::after(Duration::from_millis(200)).await;
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

        let usb_switch_state = USBSwitchState::from_leds(&led_a, &led_b);

        {
            let mut cps = control_panel_state.lock().await;
            cps.update_usb_switch_state(usb_switch_state);
            cps.display.flush().unwrap();
        }
    }
}

#[embassy_executor::task]
async fn monitor_meeting_sign_timer(control_panel_state: &'static StateMutex) {
    debug!("Starting monitor_meeting_sign_timer task");
    loop {
        match MEETING_SIGN_STATE.wait().await {
            MeetingSignState::On => {
                let mut ticker = Ticker::every(Duration::from_secs(30));
                loop {
                    let meeting_sign_completion = {
                        let mut cps = control_panel_state.lock().await;
                        cps.check_meeting_sign_timer(&MEETING_SIGN_STATE).unwrap();
                        cps.display.flush().unwrap();
                        cps.meeting_sign_completion
                    };

                    // Break out of the loop when the timer is no longer active
                    if meeting_sign_completion.is_none() {
                        info!("monitor_meeting_sign_timer - Timer completed, exiting monitoring loop at {:?}", Instant::now());
                        break;
                    }

                    ticker.next().await;
                }
            }
            MeetingSignState::Off => {
                info!("monitor_meeting_sign_timer - Meeting sign is off, continuing to wait");
                // TODO: kick off sleep timer here? Want the control panel to sleep when the
                // Meeting Sign has been Off for 5 minutes. But any other input should reset the 5
                // min timer.
            }
            MeetingSignState::Disconnected => {
                info!(
                    "monitor_meeting_sign_timer - Meeting sign is disconnected, continuing to wait"
                );
            }
        }
    }
}

#[embassy_executor::task]
async fn writer(mut uart: Uart<'static, Async>, control_panel_state: &'static StateMutex) {
    debug!("Starting UART writer task");

    // Buffers sized appropriately for the MeetingInstruction payload
    let mut serialize_buf = [0u8; meeting_instruction::MAX_PAYLOAD_SIZE];
    let mut encode_buf = [0u8; meeting_instruction::MAX_ENCODED_SIZE];

    loop {
        let payload = {
            match control_panel_state.lock().await.meeting_sign_completion {
                None => MeetingSignInstruction::Off,
                Some(end) => {
                    // If the Meeting Sign is active, use the remaining time
                    let duration_remaining =
                        Duration::from(MeetingDuration::MAX) - (end - Instant::now());
                    match MeetingDuration::new(duration_remaining) {
                        Err(_) => {
                            error!("Failed to create meeting duration: {duration_remaining:?}");
                            MeetingSignInstruction::Error
                        }
                        Ok(meeting_duration) => {
                            match meeting_instruction::ProgressRatio::from_durations(
                                &meeting_duration.into(),
                                &MeetingDuration::MAX.into(),
                            ) {
                                None => {
                                    error!(
                                        "Invalid progress ratio for meeting duration: {meeting_duration:?}"
                                    );
                                    MeetingSignInstruction::Error
                                }
                                Some(progress_ratio) => MeetingSignInstruction::On(progress_ratio),
                            }
                        }
                    }
                }
            }
        };

        // Serialize the payload
        let serialized = postcard::to_slice(&payload, &mut serialize_buf).unwrap();
        let serialized_len = serialized.len();

        // COBS encode
        let encoded_len = cobs::encode(serialized, &mut encode_buf);

        debug!("Serialized: {serialized_len} bytes, Encoded: {encoded_len} bytes");
        debug!(
            "Instruction: {:?} | Raw data: {:?} | Encoded data: {:?}",
            payload,
            &serialized,
            &encode_buf[..encoded_len]
        );

        // Send encoded data + null delimiter
        uart.write_async(&encode_buf[..encoded_len]).await.unwrap();
        uart.write_async(&[COBS_DELIMITER]).await.unwrap();
        embedded_io_async::Write::flush(&mut uart).await.unwrap();

        Timer::after(UART_COMMUNICATION_INTERVAL).await;
    }
}

#[embassy_executor::task]
async fn sleep_timer(
    low_power_peripheral: LPWR<'static>,
    rtcio_wakeup_source: RtcioWakeupSource<'static, 'static>,
    control_panel_state: &'static StateMutex,
) {
    debug!("Starting sleep_timer task");
    let mut rtc = Rtc::new(low_power_peripheral);
    loop {
        match select(
            SLEEP_TIMER_EXTENSION_SIGNAL.wait(),
            // Timer::after(Duration::from_secs(60 * 5)), // 5 minutes
            Timer::after(Duration::from_secs(10)),
        )
        .await
        {
            Either::First(_) => {
                log::warn!("Sleep timer extension signal received, resetting sleep timer.");
            }
            Either::Second(_) => {
                log::warn!("Sleep timer expired, putting control panel to sleep.");
                let x = GpioWakeupSource::new();

                {
                    let mut cps = control_panel_state.lock().await;
                    cps.display.clear(BinaryColor::Off).unwrap();
                    cps.display.flush().unwrap();
                }

                // rtc.sleep_light(&[]);
                rtc.sleep_deep(&[&rtcio_wakeup_source]);
            }
        };
    }
}
