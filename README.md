# homeautomation

In this repository you can find components for a CANBus home automation system.

## mqtt_gateway
The gateway is used to access the canbus via MQTT but it also has some very useful features
for the canbus maintainance. It features a web interface (currently unencrypted http)
which enables you to make some settings for the gateway, view modify and update all your attached
devices on the can bus and update the gateway itself. It translates can messages to MQTT
and vice versa using static implemented rules.

It can also forward all can messages to MQTT for debug purposes and can send can messages
via MQTT using the "canbus/debug/<id>" topic.

The mqtt_gateway has two extension ports to hook on up to 4 relais and/or 4 sensor boards
per port. Quite powerful for mid size stand alone applications. 

### PIN assignment

## Relais
Disclaimer: It you put mains voltage on a relais, make sure that you know what you are doing.
These components are experimental and you are in charge for your onw actions. Be safe. If
you are unsure, get an expert or leave it.

Three PCBs and one software to switch power on and off. One PCB uses solid state
relais which can only be used to switch mains voltage (~230V @ 50-60Hz). Every SSR could
handle up to 1A permanent load but six SSR share one 1A fuse for protection. Be very careful
what your load is, capacitive loads or inductive loads might blow up the SSR. However, it
can savely handle small rollershutter, heating actuations and electro-magnetic contactors.

There is also a PCB with regular relais which can handle up to 16A each. There is no fuse on
the PCB, so make sure your circuits are well protected. Even if these relais can handle more
current, be careful with heavy loads such as motors, FUs and other inductive loads that can
back inject. Note that these relais do not have a deion chamber (arc quenching chamber) and
in case of changing loads, an electro-magnetic contactor might be a better choice.

The third board in specialized for rollershutter. Two relais are cascaded. The first switches
power on and the second is meant to switch between up and down. The other relais support
software rollershutter capabilities which make sure, that up and down are not active at the
same time.

They all use the same software. Tha pinning on the ESP32 is identical. All Relais PCBs
have one extension port.

### PIN assignment

## Lightswitch
The lightswitch component is one of the more advanced circuits packed with features.

## Sensors

## UART Adapter

## CAN Header

## CAN Hub

## Use cases

## Structure

## Supply Power

## CAN / TWAI

### The connector

### CAN IDs

### Message IDs

### Types

## MQTT

MQTT has a subscribe and publish communication strategy, anyone interested in a topic
will get the message when anyone makes a publish. An MQTT Message consists of a topic
and a data part. 

### MQTT to CAN
It seems logical to use this separation for the CAN messages as well.
However, there were some thought about where to put i.e. numbers of lamps or relais.

It seems best to put the identificable devices into the topic and the numbers into the
data part. When using hexadecimal numbers, they get a leading 0x. the CANID is always
in hex, additionally the BITMASK is also hex. 

#### canbus/relais_command/&lt;CANID> &lt;NO>/&lt;STATE>/&lt;TIMEOUT>
NO depends on the amount of chained relais. 
- 0-15 are onboand relais. If the PCB has only 12 relais, 12-15 are unused. 
- 16-31 is the extender with address 0b00
- 32-47 0b01
- 48-63 0b10
- 64-79 0b11
The state is 0 for OFF, 1 for ON. The TIMEOUT is in milliseconds

#### canbus/rollershutter_command/&lt;CANID> &lt;NO>/&lt;STATE>/&lt;TIMEOUT>
NO depends on the amount of chained relais. 
- 0-7 are onboand relais. If the PCB has only 12 relais, 6-7 are unused. 
- 8-15 is the extender with address 0b00
- 16-23 0b01
- 24-31 0b10
- 32-39 0b11
The state is 0 for OFF, 1 for UP, 2 for DOWN. The TIMEOUT is in milliseconds

#### canbus/lamp_command/&lt;CANID> &lt;VALUE>/&lt;BITMASK>/&lt;BANK>
The value accepts values from 0 to 255 where 0 is OFF und 255 is the maximum brightness.
The bitmask must be given in HEX, the first byte are leds 0-7 (little endian). The bitmask
musk always be 3 bytes long
- 0xFFFFFF selects all LEDs
- 0xFF0000 seletcs LEDs 0-7
- 0x010000 selects LED 0
- 0x800000 selects LED 7
- 0x000100 seletcs LED 8

#### canbus/debug/&lt;CANID> &lt;CANMESSAGE>
The CANMESSAGE must be given in HEX, and can input every message. The first byte written
in CANMESSAGE will translate to data[0] in the can message.

### CAN to MQTT
#### canbus/relais_state/&lt;CANID> &lt;NO>/&lt;STATE>
#### canbus/rollershutter_state/&lt;CANID> &lt;NO>/&lt;STATE>
#### canbus/button/&lt;CANID> &lt;BUTTONID>/&lt;EVENT>/&lt;COUNT>
The button ID depends on the amount of buttons attached to the extension board
- 0-3 maps to onboard buttons. Thed are unusen if the PCB does not have buttons.
- 4-7 maps to the extension board with address 0b00
- 8-11 0b01
- 12-15 0x10
- 16-19 0x11

#### canbus/humidity/&lt;SENSORID> &lt;HUMITIDY>
The SENSORID is a bit difficult. It has no mapping to the CANID. The Sensor ID is the
devics UID (CHIP ID on stm32 and MAC address on ESP3) appended with the pin number or
I2C address (depending on the sensor used)
The humidiy is a relative humidity in percent.

In practice just put the sensor location in your house and the according SENSORID somewhere
where it does not get lost.
 
#### canbus/temperature/&lt;SENSORID> &lt;TEMPERATURE>
For this value comes another option for the SENSORID along. DS18B20 sensors have an
own 64 bit address with 48 unique bits. When using such a sensor, these 48 bits are
the SENSORID, for other sensors its the same as for humidity.

#### canbus/log/&lt;CANID> &lt;CANMESSAGE>

## Build 
The build is done using the esp-idf (https://github.com/espressif/esp-idf), currently
in version 5.0. The idf is a huge collection of libraries and tools which increases the
development speed a lot. The documentation for the idf can be found here: 

https://docs.espressif.com/projects/esp-idf/en/latest/esp32/api-reference/index.html

The chip reference manual is also there but it was never needed for this project.
We recommend to use a linux system as development platform.
Have a look into the getting started page in the documentation.

Quick steps:

For Arch Linux:
`sudo pacman -S --needed gcc git make flex bison gperf python cmake ninja ccache dfu-util libusb`

For Ubuntu (24.04)
`sudo sudo apt update && sudo apt upgrade -y && git build-essential`

`git clone -b v5.0 --recursive https://github.com/espressif/esp-idf.git $HOME/esp`

`cd $HOME/esp`

`./install.sh esp32`

`. $HOME/esp/export.sh # note the dot at the beginning: source the file, otherwise you dont get the environment varibles set`

Than you can change to your project dir (i.e. homeautomation/software/mqtt_gateway) and
compile.

`idf.py build`

Programming can be done by

`idf.py flash -p /dev/ttyUSB0`

You can open the debug terminal and see the logging output by

`idf.py monitor -p /dev/ttyUSB0`

Or in one step

`idf.py build flash monitor -p /dev/ttyUSB0`

Full flash erase can be done by

`idf.py flash-erase flash -p /dev/ttyUSB0`

## Debugging
Sadly, the ESP32 has to few pins to support a JTAG debugger. So we are going with printf
/ ESP_LOGI debugging here and using the serial port quite a lot. The ESP also prints a
stack trace when the CPU faults, that is also helpful but still not as good as stepping through
the code and viewing every variable on the way. To connect to the serial interface we can also
use the idf command:

idf.py monitor -p /dev/ttyUSB0

idf commands can also be chained, e.g. 

idf.py build flash monitor -p /dev/ttyUSB0

### Read out partitions

## License
This project is releases under the GNU GPL 3.0. This license is rather long and difficult
to read and interpret. However, if changes are made, we like to get contribution and for the
hardware part it is best to have everything open for the most sustainability. All schematics,
layout, mechanics, tools and software is provided to fix stuff on your own rather than throwing
it away.

## Contribution
Feel free to report issues or open a merge request.
