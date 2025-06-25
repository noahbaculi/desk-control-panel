use embedded_graphics::{
    pixelcolor::BinaryColor,
    prelude::{DrawTarget, PixelColor, Point, Primitive},
    primitives::{Line, Polyline, PrimitiveStyle},
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
// impl USBSwitch {
//     pub fn draw_ui(&self) -> {
//         if self.led_a.is_high() && self.led_b.is_high() {
//             USBSwitchState::Off
//         } else if self.led_a.is_high() {
//             USBSwitchState::Output(USBSwitchOutput::A)
//         } else if self.led_b.is_high() {
//             USBSwitchState::Output(USBSwitchOutput::B)
//         } else {
//             USBSwitchState::Error
//         }
//     }
// }
impl USBSwitch {
    const CORE_TOP: Point = Point::new(6, 9);
    const CORE_BOTTOM: Point = Point::new(6, 42);
    const USB_SWITCH_THICKNESS: u32 = 2;
    const ARROW_DX: i32 = 4;
    const ARROW_DY: i32 = 4;
    const USB_SWITCH_STYLE: PrimitiveStyle<BinaryColor> =
        PrimitiveStyle::with_stroke(BinaryColor::On, Self::USB_SWITCH_THICKNESS);

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
}
impl Drawable for USBSwitch {
    type Color = BinaryColor;
    type Output = ();

    fn draw<D>(&self, target: &mut D) -> Result<Self::Output, D::Error>
    where
        D: DrawTarget<Color = BinaryColor>,
    {
        Line::new(Self::CORE_TOP, Self::CORE_BOTTOM)
            .into_styled(Self::USB_SWITCH_STYLE)
            .draw(target)?;

        match self.led_a.level() {
            Level::High => {
                Polyline::new(&Self::USB_A_POINTS)
                    .into_styled(Self::USB_SWITCH_STYLE)
                    .draw(target)?;
            }
            Level::Low => {
                Polyline::new(&Self::USB_B_POINTS)
                    .into_styled(Self::USB_SWITCH_STYLE)
                    .draw(target)?;
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
