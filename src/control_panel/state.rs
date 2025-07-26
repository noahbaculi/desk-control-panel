use crate::meeting_instruction::ProgressRatio;
use core::fmt::Write;
use embassy_time::{Duration, Instant};
use embedded_graphics::{
    mono_font::{ascii, MonoFont, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::{DrawTarget, Point, Primitive, Size},
    primitives::{
        Line, Polyline, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle, RoundedRectangle,
        StrokeAlignment, StyledDrawable,
    },
    text::{Alignment, Baseline, Text, TextStyle, TextStyleBuilder},
    Drawable,
};
use esp_hal::{
    gpio::{Input, Level, Output},
    i2c::master::I2c,
    Blocking,
};
use heapless::String;
use log::{error, info};
use ssd1306::{
    mode::BufferedGraphicsMode, prelude::I2CInterface, size::DisplaySize128x64, Ssd1306,
};

type DisplayType = Ssd1306<
    I2CInterface<I2c<'static, Blocking>>,
    DisplaySize128x64,
    BufferedGraphicsMode<DisplaySize128x64>,
>;
pub struct ControlPanelState {
    pub usb_switch_state: USBSwitchState,
    pub usb_power_1: Output<'static>,
    pub usb_power_2: Output<'static>,
    pub meeting_sign_power: Output<'static>,
    pub meeting_sign_completion: Option<Instant>,
    pub ui_selection_mode: UISelectionMode,
    pub ui_section: UISection,
    pub display: DisplayType,
}

const MEETING_SIGN_INTERVAL: Duration = Duration::from_secs(60 * 5);
const MEETING_SIGN_MAX_DURATION: Duration = Duration::from_secs(60 * 120);

pub enum MovementDirection {
    Clockwise,
    CounterClockwise,
}
impl ControlPanelState {
    pub fn rotary_encoder_rotate(&mut self, direction: MovementDirection) {
        match self.ui_selection_mode {
            UISelectionMode::Menu => {
                match direction {
                    MovementDirection::Clockwise => self.ui_section = self.ui_section.next(),
                    MovementDirection::CounterClockwise => self.ui_section = self.ui_section.prev(),
                };
                self.draw_border_ui().unwrap();
            }
            UISelectionMode::Selected => match self.ui_section {
                UISection::USBPower1 => {
                    self.usb_power_1.toggle();
                    USBPowerMosfet::One
                        .draw(&mut self.display, self.usb_power_1.output_level())
                        .unwrap();
                }
                UISection::USBPower2 => {
                    self.usb_power_2.toggle();
                    USBPowerMosfet::Two
                        .draw(&mut self.display, self.usb_power_2.output_level())
                        .unwrap();
                }
                UISection::MeetingSign => {
                    self.process_meeting_sign_change(direction).unwrap();
                }
            },
        };

        self.display.flush().unwrap();
    }

    fn process_meeting_sign_change(
        &mut self,
        direction: MovementDirection,
    ) -> Result<(), <DisplayType as DrawTarget>::Error> {
        let now = Instant::now();

        match (direction, self.meeting_sign_completion) {
            (MovementDirection::Clockwise, None) => {
                self.meeting_sign_power.set_high();
                self.meeting_sign_completion = Some(now + MEETING_SIGN_INTERVAL);
                self.check_meeting_sign_timer()?;
                info!("Starting meeting sign timer from None");
            }
            (MovementDirection::Clockwise, Some(end)) => {
                // If the stored end is before (less than) now, we use now as the base time
                let proposed_end = end.max(now) + MEETING_SIGN_INTERVAL;

                if proposed_end > now + MEETING_SIGN_MAX_DURATION {
                    info!("Meeting sign timer would exceed max duration, not increasing");
                    return Ok(());
                }

                self.meeting_sign_power.set_high();
                self.meeting_sign_completion = Some(proposed_end);
                self.check_meeting_sign_timer()?;
                info!("Meeting sign timer increased",);
            }
            (MovementDirection::CounterClockwise, None) => {
                info!("Meeting sign is already off, nothing to do");
            }
            (MovementDirection::CounterClockwise, Some(end)) => {
                if end - MEETING_SIGN_INTERVAL < now {
                    self.meeting_sign_power.set_low();
                    self.meeting_sign_completion = None;
                    info!("Meeting sign turned off and completion set to None");
                } else {
                    self.meeting_sign_completion = Some(end - MEETING_SIGN_INTERVAL);
                    info!(
                        "Decreasing Meeting Sign timer by {}s.",
                        MEETING_SIGN_INTERVAL.as_secs()
                    );
                }
                self.check_meeting_sign_timer()?;
            }
        };
        Ok(())
    }

    pub fn rotary_encoder_press(&mut self) {
        match self.ui_selection_mode {
            UISelectionMode::Menu => {
                self.ui_selection_mode = UISelectionMode::Selected;
            }
            UISelectionMode::Selected => {
                self.ui_selection_mode = UISelectionMode::Menu;
            }
        };
        self.draw_border_ui().unwrap();
        self.display.flush().unwrap();
    }

    fn draw_selected_border_ui(
        &mut self,
        rounded_rectangle: RoundedRectangle,
    ) -> Result<(), <DisplayType as DrawTarget>::Error> {
        let target = &mut self.display;

        rounded_rectangle.draw_styled(&UISection::BORDER_OFF_STYLE, target)?;
        match self.ui_selection_mode {
            UISelectionMode::Menu => {
                // Draw dashed border
                for pixel in rounded_rectangle
                    .into_styled(UISection::BORDER_ON_STYLE)
                    .pixels()
                    .step_by(3)
                {
                    pixel.draw(target)?;
                }
            }
            UISelectionMode::Selected => {
                rounded_rectangle.draw_styled(&UISection::BORDER_ON_STYLE, target)?;
            }
        }
        Ok(())
    }

    pub fn draw_border_ui(&mut self) -> Result<(), <DisplayType as DrawTarget>::Error> {
        let target = &mut self.display;
        match self.ui_section {
            UISection::USBPower1 => {
                UISection::MEETING_SIGN_BORDER.draw_styled(&UISection::BORDER_OFF_STYLE, target)?;
                UISection::USB_POWER_2_BORDER.draw_styled(&UISection::BORDER_OFF_STYLE, target)?;
                self.draw_selected_border_ui(UISection::USB_POWER_1_BORDER)?;
            }
            UISection::USBPower2 => {
                UISection::MEETING_SIGN_BORDER.draw_styled(&UISection::BORDER_OFF_STYLE, target)?;
                UISection::USB_POWER_1_BORDER.draw_styled(&UISection::BORDER_OFF_STYLE, target)?;
                self.draw_selected_border_ui(UISection::USB_POWER_2_BORDER)?;
            }
            UISection::MeetingSign => {
                UISection::USB_POWER_1_BORDER.draw_styled(&UISection::BORDER_OFF_STYLE, target)?;
                UISection::USB_POWER_2_BORDER.draw_styled(&UISection::BORDER_OFF_STYLE, target)?;
                self.draw_selected_border_ui(UISection::MEETING_SIGN_BORDER)?;
            }
        };

        Ok(())
    }

    pub fn update_usb_switch_state(
        &mut self,
        usb_switch_state: USBSwitchState,
    ) -> Result<(), <DisplayType as DrawTarget>::Error> {
        self.usb_switch_state = usb_switch_state;
        self.usb_switch_state.draw(&mut self.display)?;
        self.display.flush()?;
        Ok(())
    }

    pub fn check_meeting_sign_timer(&mut self) -> Result<(), <DisplayType as DrawTarget>::Error> {
        let now = Instant::now();
        let mut remaining = None;

        match (
            self.meeting_sign_power.output_level(),
            self.meeting_sign_completion,
        ) {
            (Level::Low, None) => {}
            (Level::Low, Some(_)) => {
                error!("Meeting Sign is not on, but completion is set to something");
                // This should not happen, but if it does, we can reset the state
                self.meeting_sign_completion = None;
            }
            (Level::High, None) => {
                error!("Meeting Sign is on, but completion is None");
                // This should not happen, but if it does, we can reset the state
                self.meeting_sign_power.set_low();
                self.meeting_sign_completion = None;
            }
            (Level::High, Some(end)) => {
                if end < now {
                    self.meeting_sign_power.set_low();
                    self.meeting_sign_completion = None;
                    info!("Meeting Sign timer has completed");
                } else {
                    let ratio =
                        ProgressRatio::from_durations(&(end - now), &MEETING_SIGN_MAX_DURATION)
                            .unwrap();
                    remaining = Some(end - now);
                    info!(
                        "Meeting Sign timer is running with ratio={ratio:?}, remaining={}s",
                        remaining.unwrap().as_secs()
                    );
                }
            }
        };

        MeetingSignUI.draw_progress(&mut self.display, remaining)?;

        Ok(())
    }

    pub fn draw_entire_ui(&mut self) -> Result<(), <DisplayType as DrawTarget>::Error> {
        self.draw_border_ui()?;
        self.usb_switch_state.draw_entire_ui(&mut self.display)?;
        USBPowerMosfet::USB_POWER_1_TEXT.draw(&mut self.display)?;
        USBPowerMosfet::USB_POWER_2_TEXT.draw(&mut self.display)?;
        USBPowerMosfet::One.draw(&mut self.display, self.usb_power_1.output_level())?;
        USBPowerMosfet::Two.draw(&mut self.display, self.usb_power_2.output_level())?;
        MeetingSignUI::TITLE_TEXT.draw(&mut self.display)?;
        MeetingSignUI::FULL_PROGRESS_SHAPE
            .draw_styled(&MeetingSignUI::BORDER_ON_STYLE, &mut self.display)?;

        self.display.flush()?;
        Ok(())
    }
}

#[derive(Debug)]
pub enum USBSwitchState {
    On(USBSwitchOutput),
    Off,
}
impl USBSwitchState {
    pub fn from_leds(a: &Input<'static>, b: &Input<'static>) -> Self {
        match (a.level(), b.level()) {
            (Level::Low, Level::Low) | (Level::High, Level::High) => Self::Off,
            (Level::High, Level::Low) => Self::On(USBSwitchOutput::A),
            (Level::Low, Level::High) => Self::On(USBSwitchOutput::B),
        }
    }

    const ARROW_DX: i32 = 4;
    const ARROW_DY: i32 = 4;
    const STROKE_THICKNESS: u32 = 2;
    const RIGHT_PADDING: i32 = 8;
    const TOP_PADDING: i32 = 5;
    pub const UI_X: i32 = (Self::ARROW_DX * 2) + Self::RIGHT_PADDING;
    const CORE_TOP: Point = Point::new(Self::ARROW_DX, Self::TOP_PADDING);
    const CORE_BOTTOM: Point = Point::new(Self::ARROW_DX, 64 - Self::TOP_PADDING);

    const ON_STYLE: PrimitiveStyle<BinaryColor> =
        PrimitiveStyle::with_stroke(BinaryColor::On, Self::STROKE_THICKNESS);
    const OFF_STYLE: PrimitiveStyle<BinaryColor> =
        PrimitiveStyle::with_stroke(BinaryColor::Off, Self::STROKE_THICKNESS);

    const USB_A_POINTS: [Point; 4] = [
        Point::new(
            Self::CORE_TOP.x - Self::ARROW_DX,
            Self::CORE_TOP.y + Self::ARROW_DY,
        ),
        Self::CORE_TOP,
        Point::new(Self::CORE_TOP.x + 1, Self::CORE_TOP.y),
        Point::new(
            Self::CORE_TOP.x + Self::ARROW_DX + 1,
            Self::CORE_TOP.y + Self::ARROW_DY,
        ),
    ];
    const USB_B_POINTS: [Point; 4] = [
        Point::new(
            Self::CORE_BOTTOM.x - Self::ARROW_DX,
            Self::CORE_BOTTOM.y - Self::ARROW_DY,
        ),
        Self::CORE_BOTTOM,
        Point::new(Self::CORE_BOTTOM.x + 1, Self::CORE_BOTTOM.y),
        Point::new(
            Self::CORE_BOTTOM.x + Self::ARROW_DX + 1,
            Self::CORE_BOTTOM.y - Self::ARROW_DY,
        ),
    ];
    const ERROR_GRAPHIC: Line = Line::new(Self::CORE_TOP, Self::CORE_BOTTOM);

    const Y_MIDDLE: i32 = 64 / 2;
    const TEXT_FONT: MonoFont<'_> = ascii::FONT_8X13;
    const STYLE: MonoTextStyle<'_, BinaryColor> =
        MonoTextStyle::new(&Self::TEXT_FONT, BinaryColor::On);
    const CENTER_ALIGNED: TextStyle = TextStyleBuilder::new()
        .alignment(Alignment::Center)
        .baseline(Baseline::Middle)
        .build();
    const TEXT_U: Text<'_, MonoTextStyle<'_, BinaryColor>> = Text::with_text_style(
        "U",
        Point::new(
            Self::CORE_TOP.x,
            Self::Y_MIDDLE - Self::TEXT_FONT.character_size.height as i32,
        ),
        Self::STYLE,
        Self::CENTER_ALIGNED,
    );
    const TEXT_S: Text<'_, MonoTextStyle<'_, BinaryColor>> = Text::with_text_style(
        "S",
        Point::new(Self::CORE_TOP.x, Self::Y_MIDDLE),
        Self::STYLE,
        Self::CENTER_ALIGNED,
    );
    const TEXT_B: Text<'_, MonoTextStyle<'_, BinaryColor>> = Text::with_text_style(
        "B",
        Point::new(
            Self::CORE_TOP.x,
            Self::Y_MIDDLE + Self::TEXT_FONT.character_size.height as i32,
        ),
        Self::STYLE,
        Self::CENTER_ALIGNED,
    );

    pub fn draw_entire_ui<D: DrawTarget<Color = BinaryColor>>(
        &self,
        target: &mut D,
    ) -> Result<(), D::Error> {
        Self::TEXT_U.draw(target)?;
        Self::TEXT_S.draw(target)?;
        Self::TEXT_B.draw(target)?;

        self.draw(target)?;

        Ok(())
    }
}
impl Drawable for USBSwitchState {
    type Color = BinaryColor;
    type Output = ();

    fn draw<D>(&self, target: &mut D) -> Result<Self::Output, D::Error>
    where
        D: DrawTarget<Color = BinaryColor>,
    {
        match self {
            Self::Off => {
                Polyline::new(&Self::USB_A_POINTS).draw_styled(&Self::OFF_STYLE, target)?;
                Polyline::new(&Self::USB_B_POINTS).draw_styled(&Self::OFF_STYLE, target)?;

                Self::ERROR_GRAPHIC
                    .draw_styled(&PrimitiveStyle::with_stroke(BinaryColor::On, 1), target)?;
            }
            Self::On(USBSwitchOutput::A) => {
                Self::ERROR_GRAPHIC
                    .draw_styled(&PrimitiveStyle::with_stroke(BinaryColor::Off, 1), target)?;
                Polyline::new(&Self::USB_B_POINTS).draw_styled(&Self::OFF_STYLE, target)?;

                Polyline::new(&Self::USB_A_POINTS).draw_styled(&Self::ON_STYLE, target)?;
            }
            Self::On(USBSwitchOutput::B) => {
                Self::ERROR_GRAPHIC
                    .draw_styled(&PrimitiveStyle::with_stroke(BinaryColor::Off, 1), target)?;
                Polyline::new(&Self::USB_A_POINTS).draw_styled(&Self::OFF_STYLE, target)?;

                Polyline::new(&Self::USB_B_POINTS).draw_styled(&Self::ON_STYLE, target)?;
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
pub enum USBSwitchOutput {
    A,
    B,
}

#[derive(Debug)]
pub enum MeetingSignState {
    On,
    Off,
    Disconnected,
}

#[derive(Debug)]
pub enum USBPowerMosfet {
    One,
    Two,
}
impl USBPowerMosfet {
    const PADDING_X: u32 = 4;
    const ON_INDICATOR: Size = Size::new(
        UISection::USB_POWER_SIZE.width - (Self::PADDING_X * 2),
        UISection::USB_POWER_SIZE.height - (Self::PADDING_X * 2) - 10,
    );

    const ON_STYLE: PrimitiveStyle<BinaryColor> = PrimitiveStyle::with_fill(BinaryColor::On);
    const OFF_STYLE: PrimitiveStyle<BinaryColor> = PrimitiveStyle::with_stroke(BinaryColor::On, 1);
    const CLEAR_STYLE: PrimitiveStyle<BinaryColor> = PrimitiveStyle::with_fill(BinaryColor::Off);

    const BORDER_RADIUS: Size = Size::new(3, 3);
    const USB_POWER_1: RoundedRectangle = RoundedRectangle::with_equal_corners(
        Rectangle::new(
            Point::new(
                UISection::USB_POWER_X + Self::PADDING_X as i32,
                Self::PADDING_X as i32,
            ),
            Self::ON_INDICATOR,
        ),
        Self::BORDER_RADIUS,
    );
    const USB_POWER_2: RoundedRectangle = RoundedRectangle::with_equal_corners(
        Rectangle::new(
            Point::new(
                UISection::USB_POWER_X + Self::PADDING_X as i32,
                UISection::USB_POWER_SIZE.height as i32 + Self::PADDING_X as i32,
            ),
            Self::ON_INDICATOR,
        ),
        Self::BORDER_RADIUS,
    );

    const STYLE: MonoTextStyle<'_, BinaryColor> =
        MonoTextStyle::new(&ascii::FONT_4X6, BinaryColor::On);
    const CENTER_ALIGNED: TextStyle = TextStyleBuilder::new()
        .alignment(Alignment::Center)
        .baseline(Baseline::Middle)
        .build();
    const USB_POWER_1_TEXT: Text<'_, MonoTextStyle<'_, BinaryColor>> = Text::with_text_style(
        "PWR 1",
        Point::new(
            UISection::USB_POWER_X + (UISection::USB_POWER_SIZE.width as i32 / 2),
            (Self::ON_INDICATOR.height + (2 * Self::PADDING_X) + 2) as i32,
        ),
        Self::STYLE,
        Self::CENTER_ALIGNED,
    );
    const USB_POWER_2_TEXT: Text<'_, MonoTextStyle<'_, BinaryColor>> = Text::with_text_style(
        "PWR 2",
        Point::new(
            UISection::USB_POWER_X + (UISection::USB_POWER_SIZE.width as i32 / 2),
            (UISection::USB_POWER_SIZE.height
                + Self::ON_INDICATOR.height
                + (2 * Self::PADDING_X)
                + 2) as i32,
        ),
        Self::STYLE,
        Self::CENTER_ALIGNED,
    );

    pub fn draw<D: DrawTarget<Color = BinaryColor>>(
        &self,
        target: &mut D,
        power: Level,
    ) -> Result<(), D::Error> {
        let shape = match self {
            USBPowerMosfet::One => Self::USB_POWER_1,
            USBPowerMosfet::Two => Self::USB_POWER_2,
        };

        // NOTE: Since this is a P-Channel MOSFET, the MOSFET is "on" when the gate is low.
        let style = match power {
            Level::High => Self::OFF_STYLE,
            Level::Low => Self::ON_STYLE,
        };
        shape.draw_styled(&Self::CLEAR_STYLE, target)?;
        shape.draw_styled(&style, target)?;
        Ok(())
    }
}

#[derive(Debug)]
pub enum UISelectionMode {
    Menu,
    Selected,
}
impl UISelectionMode {
    pub const fn toggle(&mut self) {
        *self = match self {
            UISelectionMode::Menu => UISelectionMode::Selected,
            UISelectionMode::Selected => UISelectionMode::Menu,
        };
    }
}

#[derive(Clone, Debug)]
pub enum UISection {
    MeetingSign,
    USBPower2,
    USBPower1,
}
impl UISection {
    pub const fn next(&self) -> Self {
        match self {
            UISection::USBPower1 => UISection::MeetingSign,
            UISection::USBPower2 => UISection::USBPower1,
            UISection::MeetingSign => UISection::USBPower2,
        }
    }

    pub const fn prev(&self) -> Self {
        match self {
            UISection::USBPower1 => UISection::USBPower2,
            UISection::USBPower2 => UISection::MeetingSign,
            UISection::MeetingSign => UISection::USBPower1,
        }
    }

    const BORDER_RADIUS: Size = Size::new(4, 4);
    const BORDER_WIDTH: u32 = 1;
    pub const BORDER_ON_STYLE: PrimitiveStyle<BinaryColor> =
        PrimitiveStyle::with_stroke(BinaryColor::On, Self::BORDER_WIDTH);
    pub const BORDER_OFF_STYLE: PrimitiveStyle<BinaryColor> =
        PrimitiveStyle::with_stroke(BinaryColor::Off, Self::BORDER_WIDTH);
    const USB_POWER_X: i32 = USBSwitchState::UI_X;
    pub const USB_POWER_SIZE: Size = Size::new(27, 64 / 2);
    const MEETING_SIGN_SIZE: Size = Size::new(
        128 - Self::USB_POWER_X as u32 - Self::USB_POWER_SIZE.width + Self::BORDER_WIDTH,
        64,
    );

    pub const USB_POWER_1_BORDER: RoundedRectangle = RoundedRectangle::with_equal_corners(
        Rectangle::new(Point::new(Self::USB_POWER_X, 0), Self::USB_POWER_SIZE),
        Self::BORDER_RADIUS,
    );
    pub const USB_POWER_2_BORDER: RoundedRectangle = RoundedRectangle::with_equal_corners(
        Rectangle::new(
            Point::new(Self::USB_POWER_X, Self::USB_POWER_SIZE.height as i32),
            Self::USB_POWER_SIZE,
        ),
        Self::BORDER_RADIUS,
    );
    pub const MEETING_SIGN_BORDER: RoundedRectangle = RoundedRectangle::with_equal_corners(
        Rectangle::new(
            Point::new(
                Self::USB_POWER_X + Self::USB_POWER_SIZE.width as i32 - Self::BORDER_WIDTH as i32,
                0,
            ),
            Self::MEETING_SIGN_SIZE,
        ),
        Self::BORDER_RADIUS,
    );
}

struct MeetingSignUI;
impl MeetingSignUI {
    const MIDDLE_X: i32 = UISection::USB_POWER_X
        + (UISection::USB_POWER_SIZE.width + (UISection::MEETING_SIGN_SIZE.width / 2)
            - UISection::BORDER_WIDTH) as i32;

    const TITLE_FONT: MonoFont<'_> = ascii::FONT_6X12;
    const TITLE_STYLE: MonoTextStyle<'_, BinaryColor> =
        MonoTextStyle::new(&Self::TITLE_FONT, BinaryColor::On);
    const CENTER_ALIGNED: TextStyle = TextStyleBuilder::new()
        .alignment(Alignment::Center)
        .baseline(Baseline::Top)
        .build();
    pub const TITLE_TEXT: Text<'_, MonoTextStyle<'_, BinaryColor>> = Text::with_text_style(
        "Meeting Sign",
        Point::new(Self::MIDDLE_X, 44),
        Self::TITLE_STYLE,
        Self::CENTER_ALIGNED,
    );

    const TIME_REMAINING_FONT: MonoFont<'_> = ascii::FONT_8X13;
    const TIME_REMAINING_CHARACTERS: usize = 4;
    const TIME_REMAINING_STYLE: MonoTextStyle<'_, BinaryColor> =
        MonoTextStyle::new(&Self::TIME_REMAINING_FONT, BinaryColor::On);
    const TIME_REMAINIG_PT: Point = Point::new(Self::MIDDLE_X, 5);

    const TIME_REMAINING_SIZE: Size = Size::new(
        Self::TIME_REMAINING_FONT.character_size.width * Self::TIME_REMAINING_CHARACTERS as u32,
        Self::TIME_REMAINING_FONT.character_size.height,
    );
    const TIME_REMAINING_BOUNDING_BOX: Rectangle = Rectangle::new(
        Point::new(
            Self::TIME_REMAINIG_PT.x - (Self::TIME_REMAINING_SIZE.width as i32 / 2) + 1,
            Self::TIME_REMAINIG_PT.y,
        ),
        Size::new(
            Self::TIME_REMAINING_FONT.character_size.width * Self::TIME_REMAINING_CHARACTERS as u32,
            Self::TIME_REMAINING_FONT.character_size.height,
        ),
    );

    const BORDER_ON_STYLE: PrimitiveStyle<BinaryColor> = PrimitiveStyleBuilder::new()
        .stroke_color(BinaryColor::On)
        .stroke_width(1)
        .stroke_alignment(StrokeAlignment::Outside)
        .build();

    const PROGRESS_SIZE: Size = Size::new(UISection::MEETING_SIGN_SIZE.width - 20, 12);
    const PROGRESS_X: i32 = Self::MIDDLE_X - (Self::PROGRESS_SIZE.width as i32 / 2);
    const PROGRESS_Y: i32 = 25;
    const PROGRESS_PT: Point = Point::new(Self::PROGRESS_X, Self::PROGRESS_Y);
    const PROGRESS_CORNER_RADIUS: Size = Size::new(3, 3);
    const FULL_PROGRESS_SHAPE: RoundedRectangle = RoundedRectangle::with_equal_corners(
        Rectangle::new(Self::PROGRESS_PT, Self::PROGRESS_SIZE),
        Self::PROGRESS_CORNER_RADIUS,
    );
    const BAR_ON_STYLE: PrimitiveStyle<BinaryColor> = PrimitiveStyle::with_fill(BinaryColor::On);

    pub fn draw_progress<D: DrawTarget<Color = BinaryColor>>(
        &self,
        target: &mut D,
        remaining: Option<Duration>,
    ) -> Result<(), D::Error> {
        // Clear the interior of the progress bar
        Self::FULL_PROGRESS_SHAPE
            .draw_styled(&PrimitiveStyle::with_fill(BinaryColor::Off), target)?;

        // Clear the time remaining text area
        Self::TIME_REMAINING_BOUNDING_BOX
            .draw_styled(&PrimitiveStyle::with_fill(BinaryColor::Off), target)?;

        match remaining {
            None => {
                // If no time is remaining, do not draw the progress bar or the time remaining text
                return Ok(());
            }
            Some(remaining) => {
                // let remaining_secs = remaining.as_secs();
                let time_remaining_text = Self::format_duration_h_mm(&remaining);

                let time_remaining = Text::with_text_style(
                    &time_remaining_text,
                    Self::TIME_REMAINIG_PT,
                    Self::TIME_REMAINING_STYLE,
                    Self::CENTER_ALIGNED,
                );
                time_remaining.draw(target)?;

                let ratio =
                    ProgressRatio::from_durations(&remaining, &MEETING_SIGN_MAX_DURATION).unwrap();
                let bar_width = ratio.apply_to(Self::PROGRESS_SIZE.width as usize) as u32;

                RoundedRectangle::with_equal_corners(
                    Rectangle::new(
                        Self::PROGRESS_PT,
                        Size::new(bar_width, Self::PROGRESS_SIZE.height),
                    ),
                    Self::PROGRESS_CORNER_RADIUS,
                )
                .draw_styled(&Self::BAR_ON_STYLE, target)?;
            }
        }

        Ok(())
    }

    /// Format duration in seconds as "h:mm"
    /// Naturally, with only 4 characters this will not handle durations longer than 9 hours 59 minutes.
    fn format_duration_h_mm(duration: &Duration) -> String<{ Self::TIME_REMAINING_CHARACTERS }> {
        let mut s = String::<{ Self::TIME_REMAINING_CHARACTERS }>::new();

        let mut seconds = duration.as_secs();
        seconds += 59; // Add 59 seconds to round up to the next minute
        seconds = seconds.min(9 * 60 * 60 + 59 * 60); // Cap at 9 hours 59 minutes

        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;

        // Format as "h:mm"
        write!(&mut s, "{hours}:{minutes:02}").unwrap();

        s
    }
}
