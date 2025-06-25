use embedded_graphics::{
    pixelcolor::BinaryColor,
    prelude::{DrawTarget, PixelColor, Point, Primitive},
    primitives::{Line, Polyline, PrimitiveStyle, Triangle},
    Drawable,
};
use esp_hal::gpio::{Input, Level, Output};

#[derive(Debug)]
pub struct ControlPanelState {
    pub usb_switch: USBSwitch,
    pub usb_power_1: Output<'static>,
    pub usb_power_2: Output<'static>,
    pub meeting_sign_power: Output<'static>,
    pub ui_selection_mode: UISelectionMode,
    pub ui_section: UISection,
}
pub enum MovementDirection {
    Clockwise,
    CounterClockwise,
}
impl ControlPanelState {
    pub fn rotary_encoder_rotate(&mut self, direction: MovementDirection) {
        match self.ui_selection_mode {
            UISelectionMode::Menu => {
                self.ui_section = self.ui_section.next();
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
    const USB_SWITCH_THICKNESS: u32 = 2;
    const ARROW_DX: i32 = 4;
    const ARROW_DY: i32 = 4;
    const ON_STYLE: PrimitiveStyle<BinaryColor> =
        PrimitiveStyle::with_stroke(BinaryColor::On, Self::USB_SWITCH_THICKNESS);
    const OFF_STYLE: PrimitiveStyle<BinaryColor> =
        PrimitiveStyle::with_stroke(BinaryColor::Off, Self::USB_SWITCH_THICKNESS);

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
            (Level::Low, Level::Low) => {
                Self::CORE_LINE.into_styled(Self::OFF_STYLE).draw(target)?;
                Polyline::new(&Self::USB_A_POINTS)
                    .into_styled(Self::OFF_STYLE)
                    .draw(target)?;
                Polyline::new(&Self::USB_B_POINTS)
                    .into_styled(Self::OFF_STYLE)
                    .draw(target)?;

                Self::ERROR_GRAPHIC
                    .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
                    .draw(target)?;
            }
            (Level::High, Level::Low) => {
                Self::ERROR_GRAPHIC
                    .into_styled(PrimitiveStyle::with_stroke(BinaryColor::Off, 1))
                    .draw(target)?;
                Polyline::new(&Self::USB_B_POINTS)
                    .into_styled(Self::OFF_STYLE)
                    .draw(target)?;

                Self::CORE_LINE.into_styled(Self::ON_STYLE).draw(target)?;
                Polyline::new(&Self::USB_A_POINTS)
                    .into_styled(Self::ON_STYLE)
                    .draw(target)?;
            }
            (Level::Low, Level::High) => {
                Self::ERROR_GRAPHIC
                    .into_styled(PrimitiveStyle::with_stroke(BinaryColor::Off, 1))
                    .draw(target)?;
                Polyline::new(&Self::USB_A_POINTS)
                    .into_styled(Self::OFF_STYLE)
                    .draw(target)?;

                Self::CORE_LINE.into_styled(Self::ON_STYLE).draw(target)?;
                Polyline::new(&Self::USB_B_POINTS)
                    .into_styled(Self::ON_STYLE)
                    .draw(target)?;
            }
            (Level::High, Level::High) => todo!(),
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
    USBPower1,
    USBPower2,
    MeetingSign,
}
impl UISection {
    pub fn next(&self) -> Self {
        match self {
            UISection::USBPower1 => UISection::USBPower2,
            UISection::USBPower2 => UISection::MeetingSign,
            UISection::MeetingSign => UISection::USBPower1,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            UISection::USBPower1 => UISection::MeetingSign,
            UISection::USBPower2 => UISection::USBPower1,
            UISection::MeetingSign => UISection::USBPower2,
        }
    }
}
