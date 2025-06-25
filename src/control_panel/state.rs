use embedded_graphics::{
    pixelcolor::BinaryColor,
    prelude::{DrawTarget, Point, PointsIter, Primitive, Size},
    primitives::{
        Line, Polyline, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle, RoundedRectangle,
        StrokeAlignment, StyledDrawable, Triangle,
    },
    Drawable, Pixel,
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
pub struct ControlPanelState {
    pub usb_switch_state: USBSwitchState,
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

    pub fn rotary_encoder_press(&mut self) {
        match self.ui_selection_mode {
            UISelectionMode::Menu => {
                self.ui_selection_mode = UISelectionMode::Selected;
            }
            UISelectionMode::Selected => {
                self.ui_selection_mode = UISelectionMode::Menu;
            }
        };
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

    pub fn draw_ui(&mut self) -> Result<(), <DisplayType as DrawTarget>::Error> {
        self.draw_border_ui()?;
        self.usb_switch_state.draw(&mut self.display)?;
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
    const ARROW_DX: i32 = 4;
    const ARROW_DY: i32 = 4;
    const STROKE_THICKNESS: u32 = 2;
    const RIGHT_PADDING: i32 = 8;
    pub const UI_X: i32 = (Self::ARROW_DX * 2) + Self::RIGHT_PADDING;
    const CORE_TOP: Point = Point::new(Self::ARROW_DX, 9);
    const CORE_BOTTOM: Point = Point::new(Self::ARROW_DX, 42);

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

    pub fn from_leds(a: &Input<'static>, b: &Input<'static>) -> Self {
        match (a.level(), b.level()) {
            (Level::Low, Level::Low) | (Level::High, Level::High) => Self::Off,
            (Level::High, Level::Low) => Self::On(USBSwitchOutput::A),
            (Level::Low, Level::High) => Self::On(USBSwitchOutput::B),
        }
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
                Self::CORE_LINE.draw_styled(&Self::OFF_STYLE, target)?;
                Polyline::new(&Self::USB_A_POINTS).draw_styled(&Self::OFF_STYLE, target)?;
                Polyline::new(&Self::USB_B_POINTS).draw_styled(&Self::OFF_STYLE, target)?;

                Self::ERROR_GRAPHIC
                    .draw_styled(&PrimitiveStyle::with_stroke(BinaryColor::On, 1), target)?;
            }
            Self::On(USBSwitchOutput::A) => {
                Self::ERROR_GRAPHIC
                    .draw_styled(&PrimitiveStyle::with_stroke(BinaryColor::Off, 1), target)?;
                Polyline::new(&Self::USB_B_POINTS).draw_styled(&Self::OFF_STYLE, target)?;

                Self::CORE_LINE.draw_styled(&Self::ON_STYLE, target)?;
                Polyline::new(&Self::USB_A_POINTS).draw_styled(&Self::ON_STYLE, target)?;
            }
            Self::On(USBSwitchOutput::B) => {
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
    const BORDER_WIDTH: u32 = 1;
    pub const BORDER_ON_STYLE: PrimitiveStyle<BinaryColor> =
        PrimitiveStyle::with_stroke(BinaryColor::On, Self::BORDER_WIDTH);
    pub const BORDER_OFF_STYLE: PrimitiveStyle<BinaryColor> =
        PrimitiveStyle::with_stroke(BinaryColor::Off, Self::BORDER_WIDTH);
    const USB_POWER_X: i32 = USBSwitchState::UI_X;
    const USB_POWER_SIZE: Size = Size::new(27, 64 / 2);
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
