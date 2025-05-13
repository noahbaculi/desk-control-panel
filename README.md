# Desk Control Panel

## Design Requirements

A control panel for under my desk that allows me to control my KVM as well as other peripherals.

The main controller is an ESP32-C3.

### Peripherals

- 2 x 2IN-1OUT HDMI switch
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

#### USB Hub Switch

The USB hub switch directs 4 USB ports between upstream computer A or upstream computer B.

There are two control pins with a 4.75V potential difference. When these two control pins are bridged, the hub toggles the USB source.

There are two LEDs with 1.9V potential difference to indicate which computer is being used as the source. These can be tapped into in order to get the current state.

#### Speaker Channels

There is currently two toggle switches that are manually spliced into the 3.5mm audio cables from two computers to direct the output from each computer to the speaker left and right channels.

<!-- TODO: Should we keep this implementation or do something else with an ESP32 now? -->

#### USB Power

Control USB power using MOSFETs. Planned USB-powered peripherals include:

- Pyle PAD43MXUBT Audio Mixer (500mA @ 5V)
- Arduino LED sign (200mA @ 5V)

This should be accomplished with a P-Channel MOSFET or a USB power switch IC.

- USB switch IC: TPS2054
  - Available from DigiKey. Costs more but has more features.
- P-MOSFET: IRLML6402
  - Widely available and cheaper but fewer features like short-circuit and thermal protection.

> Using an N-Channel MOSFET is not ideal because the USB spec assumes GND is always connected and stable.

### Important Points

- Make sure to connect the grounds of all the peripherals.

