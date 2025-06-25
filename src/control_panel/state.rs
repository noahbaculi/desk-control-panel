use embedded_graphics::{
    pixelcolor::BinaryColor,
    prelude::{DrawTarget, Point, Size},
    primitives::{
        Line, Polyline, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle, RoundedRectangle,
        StrokeAlignment, StyledDrawable, Triangle,
    },
    Drawable,
};
use esp_hal::{
    gpio::{Input, Level, Output},
    i2c::master::I2c,
    Blocking,
};
use ssd1306::{
    mode::BufferedGraphicsMode, prelude::I2CInterface, size::DisplaySize128x64, Ssd1306,
};

type DisplayType = Ssd1306<
    I2CInterface<I2c<'static, Blocking>>,
    DisplaySize128x64,
    BufferedGraphicsMode<DisplaySize128x64>,
>;
// #[derive(Debug)]
pub struct ControlPanelState {
    pub usb_switch: USBSwitch,
    pub usb_power_1: Output<'static>,
    pub usb_power_2: Output<'static>,
    pub meeting_sign_power: Output<'static>,
    pub ui_selection_mode: UISelectionMode,
    pub ui_section: UISection,
    pub display: DisplayType,
}
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
                UISection::USBPower1 => self.usb_power_1.toggle(),
                UISection::USBPower2 => self.usb_power_2.toggle(),
                UISection::MeetingSign => match direction {
                    MovementDirection::Clockwise => todo!(),
                    MovementDirection::CounterClockwise => todo!(),
                },
            },
        };

        self.display.flush().unwrap();
    }

    fn draw_border_ui(&mut self) -> Result<(), <DisplayType as DrawTarget>::Error> {
        let target = &mut self.display;
        match self.ui_section {
            UISection::USBPower1 => {
                UISection::MEETING_SIGN_BORDER.draw_styled(&UISection::BORDER_OFF_STYLE, target)?;
                UISection::USB_POWER_2_BORDER.draw_styled(&UISection::BORDER_OFF_STYLE, target)?;
                UISection::USB_POWER_1_BORDER.draw_styled(&UISection::BORDER_ON_STYLE, target)?;
            }
            UISection::USBPower2 => {
                UISection::MEETING_SIGN_BORDER.draw_styled(&UISection::BORDER_OFF_STYLE, target)?;
                UISection::USB_POWER_1_BORDER.draw_styled(&UISection::BORDER_OFF_STYLE, target)?;
                UISection::USB_POWER_2_BORDER.draw_styled(&UISection::BORDER_ON_STYLE, target)?;
            }
            UISection::MeetingSign => {
                UISection::USB_POWER_1_BORDER.draw_styled(&UISection::BORDER_OFF_STYLE, target)?;
                UISection::USB_POWER_2_BORDER.draw_styled(&UISection::BORDER_OFF_STYLE, target)?;
                UISection::MEETING_SIGN_BORDER.draw_styled(&UISection::BORDER_ON_STYLE, target)?;
            }
        };

        Ok(())
    }

    pub fn draw_ui(&mut self) -> Result<(), <DisplayType as DrawTarget>::Error> {
        self.draw_border_ui()?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct USBSwitch {
    pub led_a: Input<'static>,
    pub led_b: Input<'static>,
}
impl USBSwitch {
    const CORE_TOP: Point = Point::new(6, 9);
    const CORE_BOTTOM: Point = Point::new(6, 42);
    const STROKE_THICKNESS: u32 = 2;
    const ARROW_DX: i32 = 4;
    const ARROW_DY: i32 = 4;
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
    const CORE_LINE: Line = Line::new(Self::CORE_TOP, Self::CORE_BOTTOM);

    const ERROR_GRAPHIC_TOP_MIDDLE: Point = Point::new(Self::CORE_TOP.x, 30);
    const ERROR_GRAPHIC: Triangle = Triangle::new(
        Self::ERROR_GRAPHIC_TOP_MIDDLE,
        Point::new(
            Self::ERROR_GRAPHIC_TOP_MIDDLE.x - Self::ARROW_DX,
            Self::ERROR_GRAPHIC_TOP_MIDDLE.y + 6,
        ),
        Point::new(
            Self::ERROR_GRAPHIC_TOP_MIDDLE.x + Self::ARROW_DX,
            Self::ERROR_GRAPHIC_TOP_MIDDLE.y + 6,
        ),
    );
}
impl Drawable for USBSwitch {
    type Color = BinaryColor;
    type Output = ();

    fn draw<D>(&self, target: &mut D) -> Result<Self::Output, D::Error>
    where
        D: DrawTarget<Color = BinaryColor>,
    {
        match (self.led_a.level(), self.led_b.level()) {
            (Level::Low, Level::Low) | (Level::High, Level::High) => {
                Self::CORE_LINE.draw_styled(&Self::OFF_STYLE, target)?;
                Polyline::new(&Self::USB_A_POINTS).draw_styled(&Self::OFF_STYLE, target)?;
                Polyline::new(&Self::USB_B_POINTS).draw_styled(&Self::OFF_STYLE, target)?;

                Self::ERROR_GRAPHIC
                    .draw_styled(&PrimitiveStyle::with_stroke(BinaryColor::On, 1), target)?;
            }
            (Level::High, Level::Low) => {
                Self::ERROR_GRAPHIC
                    .draw_styled(&PrimitiveStyle::with_stroke(BinaryColor::Off, 1), target)?;
                Polyline::new(&Self::USB_B_POINTS).draw_styled(&Self::OFF_STYLE, target)?;

                Self::CORE_LINE.draw_styled(&Self::ON_STYLE, target)?;
                Polyline::new(&Self::USB_A_POINTS).draw_styled(&Self::ON_STYLE, target)?;
            }
            (Level::Low, Level::High) => {
                Self::ERROR_GRAPHIC
                    .draw_styled(&PrimitiveStyle::with_stroke(BinaryColor::Off, 1), target)?;
                Polyline::new(&Self::USB_A_POINTS).draw_styled(&Self::OFF_STYLE, target)?;

                Self::CORE_LINE.draw_styled(&Self::ON_STYLE, target)?;
                Polyline::new(&Self::USB_B_POINTS).draw_styled(&Self::ON_STYLE, target)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum USBSwitchState {
    Output(USBSwitchOutput),
    Off,
    Error,
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
pub enum USBPowerState {
    On,
    Off,
}

#[derive(Debug)]
pub enum UISelectionMode {
    Menu,
    Selected,
}
impl UISelectionMode {
    pub fn toggle(&mut self) {
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
    pub fn next(&self) -> Self {
        match self {
            UISection::USBPower1 => UISection::MeetingSign,
            UISection::USBPower2 => UISection::USBPower1,
            UISection::MeetingSign => UISection::USBPower2,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            UISection::USBPower1 => UISection::USBPower2,
            UISection::USBPower2 => UISection::MeetingSign,
            UISection::MeetingSign => UISection::USBPower1,
        }
    }

    const BORDER_RADIUS: Size = Size::new(4, 4);
    pub const BORDER_ON_STYLE: PrimitiveStyle<BinaryColor> = PrimitiveStyleBuilder::new()
        .stroke_color(BinaryColor::On)
        .stroke_width(1)
        .stroke_alignment(StrokeAlignment::Inside)
        .build();
    // PrimitiveStyle::with_stroke(BinaryColor::On, 1);
    pub const BORDER_OFF_STYLE: PrimitiveStyle<BinaryColor> =
        PrimitiveStyle::with_stroke(BinaryColor::Off, 1);
    pub const USB_POWER_1_BORDER: RoundedRectangle = RoundedRectangle::with_equal_corners(
        Rectangle::new(Point::new(14, 1), Size::new(27, 31)),
        Self::BORDER_RADIUS,
    );
    pub const USB_POWER_2_BORDER: RoundedRectangle = RoundedRectangle::with_equal_corners(
        Rectangle::new(Point::new(14, 32), Size::new(27, 31)),
        Self::BORDER_RADIUS,
    );
    pub const MEETING_SIGN_BORDER: RoundedRectangle = RoundedRectangle::with_equal_corners(
        Rectangle::new(Point::new(41, 1), Size::new(87, 63)),
        Self::BORDER_RADIUS,
    );
}
