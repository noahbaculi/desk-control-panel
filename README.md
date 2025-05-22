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

There are two LEDs to indicate which computer is being used as the source. These can be tapped into in order to get the current state to be read by the ESP32 and displayed on the OLED screen.
When active, the LED has a 1.8V potential difference. However, relative to a shared GND, these are the observed voltages in the various states:

| LED A State | LED A Pin 1 | LED A Pin 2 | LED B State | LED B Pin 1 | LED B Pin 2 |
| ----------- | ----------- | ----------- | ----------- | ----------- | ----------- |
| ON          | 1.8V        | 0V          | OFF         | 0V          | 0V          |
| OFF         | 3.3V        | 3.3V        | ON          | 0V          | 1.8V        |

#### USB Hub Switch

The USB hub switch directs 4 USB ports between upstream computer A or upstream computer B.

There are two control pins with a 4.75V potential difference. When these two control pins are bridged, the hub toggles the USB source.
These will be driven by a momentary button switch.

There are two LEDs to indicate which computer is being used as the source. These can be tapped into in order to get the current state to be read by the ESP32 and displayed on the OLED screen.
When active, the LED has a 1.9V potential difference. However, relative to a shared GND, these are the observed voltages in the various states:

| LED A State | LED A Pin 1 | LED A Pin 2 | LED B State | LED B Pin 1 | LED B Pin 2 |
| ----------- | ----------- | ----------- | ----------- | ----------- | ----------- |
| ON          | 0V          | 1.9V        | OFF         | 5V          | 5V          |
| OFF         | 5V          | 5V          | ON          | 0V          | 1.9V        |

Since all pins go to 5V in some state is necessary to leverage a voltage divider in order to read the 5V signals with a 3.3V ESP32.
The voltage divider could consist of 10kΩ/20kΩ resistors:

```
V_out = V_in * (R2 / (R1 + R2))
V_out = 5V * (20kΩ / (10kΩ + 20kΩ))
      = 5V * (20,000 / 30,000)
      = 5V * 0.6667
      ≈ 3.33 V

I_total = V_in / (R1 + R2)
        = 5V / (10kΩ + 20kΩ)
        = 5V / 30kΩ
        ≈ 0.000167 A
        = 167 µA

P_total = V_in × I_total
        = 5V × 0.000167 A
        ≈ 0.000833 W
        = 0.833 mW
```

#### Speaker Channels

There is currently two toggle switches that are manually spliced into the 3.5mm audio cables from two computers to direct the output from each computer to the speaker left and right channels.
This implementation will remain the same as this simple analog switching is working well.

#### USB Power

Control USB power using MOSFETs. Planned USB-powered peripherals include:

- Pyle PAD43MXUBT Audio Mixer (500mA @ 5V)
- Arduino LED sign (200mA @ 5V)

To be triggered by an ESP32, this should be accomplished with a _logic level_ P-Channel MOSFET. The MOSFET should be logic level in order to be driven by a 3.3V ESP32 directly. The IRLML6402 is widely available and cheaper but without features like short-circuit and thermal protection of a dedicated USB Switch IC.

> Using an N-Channel MOSFET is not ideal because the USB spec assumes GND is always connected and stable.

Using the P-Channel MOSFET should include

- A 1kΩ inline series gate resistor to reduce inrush current and EMI when switching the gate. [Source](https://www.build-electronic-circuits.com/mosfet-gate-resistor/)
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
