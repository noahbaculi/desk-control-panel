# Desk Control Panel

## Design Requirements

A control panel for under my desk that allows me to control my KVM as well as other peripherals.

A MCU like an ESP32 enables a fancier interface with a 0.96" OLED screen.

### Peripherals

- 2 x HDMI switch (2IN-1OUT)
- USB hub switch
- Speaker channels (left and right)
- 3 x USB power

#### HDMI Switch

The 2 IN - 1 OUT HDMI switch has 3 control pins:

- GND
- 3.65V
- INPUT

When the INPUT is pulled to GND, the switch's output is the HDMI Input A.
When the INPUT is pulled to 3.65V, the switch's output is the HDMI Input B.
When the INPUT is floating, the switch's output seems to default to the HDMI Input B.

The INPUT control pin will be driven by a simple latching switch and a 10kΩ pull-up resistor.

#### USB Hub Switch

The USB hub switch directs 4 USB ports between upstream computer A or upstream computer B.

There are two control pins with a 4.75V potential difference. When these two control pins are bridged, the hub toggles the USB source.
These will be driven by a momentary button switch.

There are two LEDs with 1.9V potential difference to indicate which computer is being used as the source. These can be tapped into in order to get the current state to be read by the ESP32 and displayed on the OLED screen.

#### Speaker Channels

There is currently two toggle switches that are manually spliced into the 3.5mm audio cables from two computers to direct the output from each computer to the speaker left and right channels.
This implementation will remain the same as this simple analog switching is working well.

#### USB Power

Control USB power using MOSFETs. Planned USB-powered peripherals include:

- Pyle PAD43MXUBT Audio Mixer (500mA @ 5V)
- Arduino LED sign (200mA @ 5V)

To be triggered by an ESP32, this should be accomplished with a P-Channel MOSFET. The IRLML6402 is widely available and cheaper but without features like short-circuit and thermal protection of a dedicated USB Switch IC.

> Using an N-Channel MOSFET is not ideal because the USB spec assumes GND is always connected and stable.

Using the P-Channel MOSFET should include

- A 1kΩ inline series gate resistor to reduce inrush current and EMI when switching the gate.
- A 10kΩ pull-up resistor to ensure the MOSFET stays off during MCU boot/reset, while the GPIO is floating.

### Important Points

- Make sure to connect the grounds of all the peripherals.

### MCU Connections

- SENSE A for HDMI Switch 1
- SENSE B for HDMI Switch 1
- SENSE A for HDMI Switch 2
- SENSE B for HDMI Switch 2
- SENSE A for USB Switch
- SENSE B for USB Switch
- CONTROL for USB Power 1
- CONTROL for USB Power 2
- CONTROL for USB Power 3
- CONTROL for USB Power 4
- GND for HDMI Switch 1
- GND for HDMI Switch 2
