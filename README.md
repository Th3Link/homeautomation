# homeautomation

In this repository you can find components for a canbus home automation system.:

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

### Topics

### MQTT to CAN
canbus/relais_command/<CANID> <NO>/<STATE>/<TIMEOUT>
canbus/rollershutter_command/<CANID> <NO>/<STATE>/<TIMEOUT>
canbus/lamp_command/<CANID> <VALUE>/<BITMASK>
canbus/debug/<CANID> <CANMESSAGE>

### CAN to MQTT
canbus/relais_state/<CANID> <NO>/<STATE>
canbus/rollershutter_state/<CANID> <NO>/<STATE>
canbus/button/<CANID> <BUTTONID>/<EVENT>/<COUNT>
canbus/humidity/<SENSORID> <HUMITIDY>
canbus/temperature/<SENSORID> <TEMPERATURE>
canbus/log/<CANID> <CANMESSAGE>

## Build 
The build is done using the esp-idf (https://github.com/espressif/esp-idf), currently
in version 5.0. The idf is a huge collection of libraries and tools which increases the
development speed a lot. The documentation for the idf can be found here: 

https://docs.espressif.com/projects/esp-idf/en/latest/esp32/api-reference/index.html

The chip reference manual is also there but it was never needed for this project.
We recommend to use a linux system as development platform.
Have a look into the getting started page in the documentation.

Quick steps:

sudo pacman -S --needed gcc git make flex bison gperf python cmake ninja ccache dfu-util libusb

git clone --recursive https://github.com/espressif/esp-idf.git
cd esp-idf
./install.sh esp32
. ./export.sh # note the dot at the beginning: source the file, otherwise you dont get the environment varibles set

Than you can change to your project dir (i.e. homeautomation/software/mqtt_gateway) and
compile.

idf.py build

Programming can be done by

idf.py flash -p /dev/ttyUSB0

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
