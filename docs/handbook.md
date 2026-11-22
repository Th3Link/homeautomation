<p class="lead">
Wire switches, sensors and actuators into a shared CAN bus. An ESP32
gateway connects the bus to your MQTT broker over Ethernet.
</p>

<ol class="system-path" aria-label="System topology">
  <li><a href="#hardware">CAN nodes</a><span>Switches, sensors, relays</span></li>
  <li><a href="#gateway-quickstart">Gateway</a><span>CAN ↔ Ethernet</span></li>
  <li><a href="#mqtt">MQTT broker</a><span>Feeds Home Assistant or openHAB</span></li>
</ol>

## Gateway quickstart

Use a gateway with the Rust firmware, one configured [wall switch](#wall-switch),
and an MQTT broker reachable on your local network. The example uses a switch
with CAN device ID `10` and the name `kitchen`. You can verify CAN discovery
before connecting a broker.

### Connect CAN and Ethernet

1. Connect the gateway and the switch to the same CAN bus. Match `CANH`,
   `CANL` and ground on both boards, and terminate the bus at its two ends
   with 120 Ω each. The current Rust firmware runs at 50 kbit/s.
2. Connect the gateway's **Ethernet** jack to your LAN and power both
   boards using the connectors and supply rails in their schematics. CAN
   distribution boards also use RJ45 connectors. Check the schematic
   pinout before connecting one.
3. Find **CAN2MQTTBridge** in your DHCP lease list, or read the address
   from the gateway's serial output. Open `https://<gateway-address>/`.
   The gateway starts with a self-signed HTTPS certificate. The serial
   command `tls-status` shows the beginning of its SHA-256 fingerprint
   and the full public-key pin.

With factory settings, sign in with `CAN2MQTTSETUP` and `Can2MqttPass`.
Set a new username and password in **Gateway → Settings**. For a blank
gateway, the [repository build instructions](../README.md#building) cover
initial serial flashing with the board's partition table.

### Find the switch

Open **Devices** and choose **Scan**. Look for `kitchen` with type
`Button` and ID `10`. When commissioning the switch over serial,
`device-type 4` sets the type, `device-id 10` sets the ID, and
`custom-string kitchen` sets the displayed name. Select the firmware
hardware revision for the board's pinout and restart the node after setting
its type or ID.

If the switch does not appear, check power, CAN polarity, termination,
and the bitrate. For older firmware, try **Broadcast ping**. See
[CAN discovery](#can-discovery) for the difference between the two actions.

### Check delivery over MQTT

In **MQTT → Primary broker**, enter your broker address and credentials.
Enable **Connect to this broker**, save, and wait for **connected**.
To check a button event on a local broker with a plain MQTT listener,
subscribe from a machine that can reach the broker.

```sh
mosquitto_sub -h broker.example -t 'canbus/button/+' -v
```

Replace `broker.example` with your broker's hostname and use that listener's
authentication settings. Double-press button `1` on `kitchen`. The message
should be `canbus/button/kitchen 1/double/0`. The
[MQTT topic reference](../software/gateway-core/docs/protocol-mqtt.md) covers
credentials, TLS, command topics, and sensor data.

## Hardware

These images are KiCad renders of the linked layouts. Parts without a
3D model appear as footprints.

- **CAN bus** · [Gateway](#mqtt-gateway), [wall switch](#wall-switch),
  [mechanical relays](#mechanical-relay-module), [solid-state relays](#solid-state-relay-module)
- **Node extensions** · [Sensors](#sensor-extension), [PWM adapter](#pwm-adapter)
- **Wiring and service** · [CAN header](#can-header), [line hub](#can-line-hub),
  [star hub](#can-star-hub), [UART adapter](#uart-debug-adapter)

### MQTT gateway

<figure class="board">
  <img src="assets/boards/gateway.webp" alt="MQTT gateway PCB with ESP32 module, Ethernet jack footprint and UART debug pads" loading="lazy">
  <figcaption>MQTT Gateway v2.0 · 75 × 51 mm</figcaption>
</figure>

An ESP32 bridge between the CAN installation and the Ethernet LAN. The
board has a dedicated Ethernet jack, CAN connections, and a UART service
header. The `gateway-hardware` firmware provides the device list, web
configuration, MQTT transport, and gateway and node firmware updates.

[KiCad layout](../hardware/mqtt_gateway/mqtt_gateway.kicad_pcb) ·
[Schematic](../hardware/mqtt_gateway/mqtt_gateway.kicad_sch)

### Wall switch

<figure class="board">
  <img src="assets/boards/switch.webp" alt="Both sides of the wall switch PCB, with the four buttons on the left view and the CAN connector footprint and expansion headers on the right view" loading="lazy">
  <figcaption>BUTTON v3.0 ESP32 · Button side (left) and reverse (right)</figcaption>
</figure>

An ESP32 CAN node for up to four local buttons. `cancomponents` reports
single, double and triple clicks and holds. The board has a CAN connector,
UART pads, and expansion headers. The selected extension can add buttons,
environmental sensors, or PWM outputs through I²C. The PCB's silkscreen
revision and the firmware's `hwrev` setting are different identifiers.

[KiCad layout](../hardware/switchModule/switch.kicad_pcb) ·
[Schematic](../hardware/switchModule/switch.kicad_sch)

### Mechanical relay module

<figure class="board">
  <img src="assets/boards/relays.webp" alt="Mechanical relay PCB with a row of orange relays, load terminal footprints and an ESP32 module" loading="lazy">
  <figcaption>Relais v2 · 188 × 97 mm</figcaption>
</figure>

Switches loads through electromechanical relays. The pictured board has
load terminals, CAN, I²C expansion, and a 1-Wire connection for DS18B20
temperature probes. `cancomponents` supports relay operation and a
heating-controller device type on this board's relay pin mapping.
Rollershutter modes use relay pairs for power and direction, with an
interlock between up and down. The firmware also supports a dedicated
`Rollershutter` device type on hardware revision 1.

[KiCad layout](../hardware/relaisModule/relais.kicad_pcb) ·
[Schematic](../hardware/relaisModule/relais.kicad_sch)

### Solid-state relay module

<figure class="board">
  <img src="assets/boards/ssr.webp" alt="Solid-state relay PCB with ESP32, output terminal footprints and fuse holder positions" loading="lazy">
  <figcaption>SSR module · 90 × 112 mm</figcaption>
</figure>

An experimental CAN-connected design for switching AC loads without
mechanical contacts. The layout has solid-state switch footprints,
fused AC supply groups, load terminals, and low-voltage CAN and expansion
connections. The current Rust firmware has a wire-level `SSR` device
type, while its hardware startup code has no dedicated `SSR` pin mapping.
For mains wiring and load suitability, refer to the actual board schematic
and fitted components.

[KiCad layout](../hardware/ssrModule/ssr.kicad_pcb) ·
[Schematic](../hardware/ssrModule/ssr.kicad_sch)

### Sensor extension

<figure class="board">
  <img src="assets/boards/sensors.webp" alt="Sensor extension PCB with I²C parts, PIR footprint and sensor connector pads" loading="lazy">
  <figcaption>SENSORUNIT2 v1.0 · 48 × 49 mm</figcaption>
</figure>

An I²C extension for a `cancomponents` node in sensor mode. The host node
reads brightness from the VEML7700 and temperature, humidity and pressure
from a BME680 or BME280. A fitted BME680 also supplies gas readings, and an
optional SCD30 measures CO₂. The layout includes connections for PIR
presence and 1-Wire. The host's firmware controls which sensors and
night-light functions are available.

[KiCad layout](../hardware/sensors/sensors.kicad_pcb) ·
[Schematic](../hardware/sensors/sensors.kicad_sch)

### PWM adapter

<figure class="board">
  <img src="assets/boards/pwm.webp" alt="PWM adapter PCB with banks of output terminals and two PCA9685 driver positions" loading="lazy">
  <figcaption>Dual PCA9685 PWM adapter</figcaption>
</figure>

Adds dimmable outputs to a host node with the extension set to PWM.
The two PCA9685 chips provide 16 channels each. The adapter has an I²C
control connection and terminals for the outputs and their supply.
The host translates CAN lamp-group commands into PWM updates.

[KiCad layout](../hardware/pwm_adapter/pwm_adapter.kicad_pcb) ·
[Schematic](../hardware/pwm_adapter/pwm_adapter.kicad_sch)

### CAN header

<figure class="board">
  <img src="assets/boards/can-header.webp" alt="CAN header PCB showing the pin header and labels for CANH, CANL, ground, 5 V and 12 V" loading="lazy">
  <figcaption>CanConnector v1.0 · 32 × 60 mm</figcaption>
</figure>

A wiring adapter for breaking out the bus to labeled `CANH`, `CANL`,
ground, 5 V, and 12 V connections. It carries bus signals and supply
rails and needs no firmware.

[KiCad layout](../hardware/canheader/canHeader.kicad_pcb) ·
[Schematic](../hardware/canheader/canHeader.kicad_sch)

### CAN line hub

<figure class="board">
  <img src="assets/boards/can-hub-line.webp" alt="Long CAN line hub PCB showing rows of RJ45 footprints and power connections" loading="lazy">
  <figcaption>CanHubLine v1.0 · line routing</figcaption>
</figure>

Passively routes CAN through successive RJ45 positions using the send
and return pairs in the matching cabling. Use the layout and schematic
to check the port sequence and uplink pinout. The board also distributes
the 5 V and 12 V rails and needs no firmware.

[KiCad layout](../hardware/canhub/canhubline.kicad_pcb) ·
[Schematic](../hardware/canhub/canhubline.kicad_sch)

### CAN star hub

<figure class="board">
  <img src="assets/boards/can-hub-star.webp" alt="CAN star hub PCB showing RJ45 footprints on a shared CAN pair" loading="lazy">
  <figcaption>CanHubStar v1.0 · parallel ports</figcaption>
</figure>

Connects the RJ45 ports in parallel on a shared CAN pair and distributes
5 V and 12 V. This passive routing has no firmware. Keep the branch
lengths and termination appropriate for the installation. The CAN
wiring uses the project's pinout, so check it before patching cables.

[KiCad layout](../hardware/canhub/canhubstar.kicad_pcb) ·
[Schematic](../hardware/canhub/canhubstar.kicad_sch)

### UART debug adapter

<figure class="board">
  <img src="assets/boards/debug.webp" alt="UART debug adapter PCB with TXD, RXD, RTS, DTR, 3.3 V and ground markings" loading="lazy">
  <figcaption>Debug adapter · 22 × 36 mm</figcaption>
</figure>

Connects to a board's service header for serial logs, commissioning and
flashing. The schematic shows the 3.3 V UART signals and the RTS/DTR
circuit for the ESP32's boot and reset lines. It needs no firmware.

[KiCad layout](../hardware/debug_adapter/debug_adapter.kicad_pcb) ·
[Schematic](../hardware/debug_adapter/debug_adapter.kicad_sch)

## CAN protocol

The firmware uses classic CAN with **29-bit extended identifiers** and
up to **8 payload bytes**. Short, 11-bit identifiers are ignored by the
gateway. A data frame carries a command or a report. A remote frame
(`R` or RTR in a trace) requests a response.

| Bits | Field | How to read it |
|---|---|---|
| 28 | `is_ng` | Next-generation protocol marker. The Rust gateway sets it to `1`. |
| 27–22 | `group` | Reserved. Normally `0`. |
| 21–16 | `device_type` | For example, `4` is a button module. |
| 15–8 | `device_id` | Node ID. `0` is used for broadcast. |
| 7–0 | `msg_type` | The message selects the payload layout. |

Clear the last byte to get the **device key** used by the gateway's
registry. The same device key appears on messages of different types
from the same node.

### Decode a button event

This frame from button `1` on the `kitchen` switch uses SocketCAN notation.

```text
10040A1E#01040000
```

- `10040A1E` is an extended ID. The marker is set, the group is `0`,
  the type is `0x04` (`Button`), the ID is `0x0A` (10), and
  `0x1E` (30) is `ButtonEvent`. Its device key is `0x10040A00`.
- `01 04 00 00` is button `1`, event `4` (`Double`), and the
  16-bit little-endian count `0`. The gateway publishes the body
  `1/double/0` on `canbus/button/kitchen`.

This example follows the
[button test fixture](../software/sim/tests/test_buttons.py#L5-L12)
and the [firmware encoder](../software/cancomponents-core/src/button_message.rs).
See the [wire format reference](#can-wire-format-reference) for field
layouts, device and message codes, and additional frame types.

## CAN discovery

CAN discovery fills the gateway's device registry with each node's CAN
identity and configured name. A node announces itself on boot, after
its name changes, or in reply to a scan. The gateway sends a scan when
it starts. You can also start one from **Devices → Scan**, the gateway's
`discover` console command, or `POST /api/v1/bus/discover`.

| Order | Frame for the example installation | What it does |
|---|---|---|
| 1 | `1000002B#R` | The gateway broadcasts a `Discovery` request to ask nodes to announce. |
| 2 | `10040A2B#6B69746368656E` | Button node 10 announces its name, `kitchen`, as seven UTF-8 bytes. |
| 3 | `10040A0C#R` | On first sighting, the gateway requests additional parameters such as the node's firmware version. |
| 4 | `10040A2C#` | The gateway sends an empty `DiscoveryAck` addressed to the node. |

Nodes spread their first announcements across a random 0–2 second delay.
For the first five attempts they wait a random 300 ms–2 s between
announcements, then 2–5 s until an acknowledgement arrives. The gateway
can show the name immediately, while the remaining parameters arrive
separately. The delays spread traffic when several nodes start together.

Older nodes can answer **Broadcast ping** (a legacy `Available` request frame)
even when **Scan** does not find them. CAN discovery travels over the
CAN bus. Home Assistant discovery is a separate step in which the
gateway publishes retained configurations over MQTT.

[CAN discovery design](adr/0021-can-discovery.md) ·
[Home Assistant MQTT discovery](#home-assistant-mqtt-discovery)

## MQTT

The gateway translates CAN events into MQTT messages and MQTT commands
back into CAN frames. It uses MQTT 5 and publishes node events, sensor
readings, and gateway announcements on the primary broker. An optional
secondary broker can also send commands to the gateway.

Set the primary broker in the gateway's **MQTT** page or with the serial
commands `mqtt-uri` and `mqtt-enabled on`. Use `mqtt://host:1883` for
a plain listener or `mqtts://host:8883` for TLS. The
[MQTT reference](../software/gateway-core/docs/protocol-mqtt.md#connection)
covers credentials and broker certificate verification.

### Topics and device addresses

Node topics use the node's configured name or, when it has none, its CAN
device key in the form `0x10040a00`. On a broker shared by several
gateways, give nodes distinct names and distinct CAN device keys across
buses. Both gateways receive matching MQTT commands. With distinct
names, only the gateway that knows the named node resolves the command
and sends it on its bus. A command with a numeric CAN key is forwarded
by every subscribed gateway. Home Assistant also uses the CAN device
key to identify a device on the shared broker.

| Direction | Example topic | Payload |
|---|---|---|
| Node → broker | `canbus/button/tastera` | `1/double/0` means button index 1, double press, count 0. |
| Node → broker | `canbus/temperature/tastera/0x68000077` | A temperature such as `27.25`, in °C. |
| Broker → node | `canbus/relais_command/relaya` | `0/3/0` switches channel 0 on. The fields are channel, state, and stop time in milliseconds. Stop time 0 leaves it on until another command. |
| Node → broker | `canbus/relais_state/relaya/0/0` | The resulting state for bank 0, channel 0. The Rust node reports `3` for on and `0` for off. |

With the [two-gateway demo](#run-the-mqtt-demo) running, subscribe in a
second terminal and press button 1 twice on `tastera` to see the event.
The second subscription also shows the response to a relay command.

```sh
mosquitto_sub -h 127.0.0.1 -p 18883 -v \
  -t 'canbus/button/+' -t 'canbus/relais_state/+/+/+'
```

From the simulation shell, send the button input and switch the simulated
relay on and off. Publish these commands without the retain flag so the
broker does not replay them when a gateway reconnects.

```sh
./sim -f "$demo/mqtt.toml" press tastera 1 double
mosquitto_pub -h 127.0.0.1 -p 18883 -t canbus/relais_command/relaya -m '0/3/0'
mosquitto_pub -h 127.0.0.1 -p 18883 -t canbus/relais_command/relaya -m '0/0/0'
```

## Home Assistant MQTT discovery

1. Add the **MQTT** integration in **Settings → Devices & services** and
   connect it to the same broker as the gateway's primary connection.
2. Keep the integration's discovery prefix at `homeassistant`. On the
   gateway, enable discovery under **MQTT → Home Assistant** or run
   `ha-discovery on` in its serial console.
3. Wait for a sensor reading or press a node's button. Open the MQTT
   integration's devices in Home Assistant. For a button, create an
   automation with a **Device** trigger and select the node.

For example, `tastera` has CAN device key `0x10040a00`. Its BME680
temperature appears after the first reading. A double press of button
index 1 also makes a **Button 2** double-press trigger available.

| What Home Assistant sees | Discovery configuration topic | Live state or event |
|---|---|---|
| Temperature sensor | `homeassistant/sensor/canbus_10040a00/temperature_68000077/config` | `canbus/temperature/tastera/0x68000077` |
| Double-press device trigger | `homeassistant/device_automation/canbus_10040a00/button1_double/config` | `canbus/button/tastera`, matching `1/double` after the value template removes the count |

The gateway advertises sensors, motion binary sensors, and device
triggers for single, double and triple presses, hold, and release after
a hold. Use the MQTT command and state topics directly for relays,
rollershutters, lamps, and heating.

The configuration topics are retained. A new subscriber gets the
stored configuration immediately and reads subsequent measurements
from the state topic. Device IDs use the CAN key, for example
`canbus_10040a00`. The temperature sensor's unique ID also includes the
sensor kind and sub-ID, giving `canbus_10040a00_temperature_68000077`.
Changing a node's display name updates its state topic and configuration
while keeping these IDs. The gateway republishes configurations after
reconnecting to the primary broker or receiving a live `online` message
on `homeassistant/status`.

Disabling gateway discovery stops updates. To remove a stored
configuration, disable its publisher and publish an empty retained
message to that specific config topic. See the
[Home Assistant MQTT discovery reference](https://www.home-assistant.io/integrations/mqtt/#mqtt-discovery)
and the [gateway design notes](../software/gateway-core/docs/adr/0022-home-assistant-discovery.md).

## Firmware updates

Use the application `.bin` for your device. The gateway and ESP32 CAN
nodes use different images. For an older STM32 node, use the image for
that board and bootloader.

| Target | Rust [release](https://github.com/Th3Link/homeautomation/releases) asset name | Maximum upload |
|---|---|---|
| Rust gateway | `gateway-hardware-<tag>.bin` | 7.75 MiB |
| ESP32 CAN node | `cancomponents-hardware-<tag>.bin` | 1 MiB |

If you build the firmware yourself, export the application image from
the newly linked [release ELF](../README.md#building). Run these commands
from the repository root after building the respective crates.

```sh
release=target/xtensa-esp32-none-elf/release
espflash save-image --chip esp32 "$release/gateway-hardware" gateway-hardware.bin
espflash save-image --chip esp32 "$release/cancomponents" cancomponents.bin
```

Use the raw application output for OTA. Use the ELF or a merged flash
image for serial flashing.

### Update the gateway

1. Sign in to the gateway and record the **Firmware** version on the
   **Gateway** page.
2. Under **Gateway firmware**, select the gateway `.bin` and click
   **Install and restart**. Keep power connected until the gateway comes back.
3. Reopen the page and check the version. Check that your settings and
   devices are still present.

The gateway writes the upload to its inactive OTA slot and checks its
length and CRC-32 before marking it for the next boot. It confirms the
running slot during startup. Settings and automations live in separate
flash partitions and survive OTA. If the gateway rejects the upload,
check the image and try again. If it no longer starts, reconnect over
USB serial and flash the gateway ELF with the
[gateway partition table](../software/gateway-hardware/partitions.csv).
From `software/gateway-hardware`, use the release ELF or an ELF from a
known working build.

```sh
espflash flash --chip esp32 --flash-size 16mb --partition-table ./partitions.csv \
  --erase-parts otadata --monitor /path/to/gateway-hardware.elf
```

This clears the OTA boot selection and preserves settings when the
existing data partitions match the table. Use this serial procedure for
the first migration of a 16 MiB gateway from the original C++ firmware.
OTA cannot install the new partition table.

### Update one CAN node

1. Open **Devices**, select the intended node, and note its name, ID, and
   version. Under **Firmware**, select the matching `.bin` and click
   **Send to this device**.
2. Allow several minutes at 50 kbit/s. The gateway sends one block at a
   time, waits for the node's acknowledgement, and retries a block if
   frames or the acknowledgement are lost. The progress shows bytes the
   node has acknowledged and the number of retries.
3. After the transfer, wait for the node to restart and reappear. Use
   **Refresh** on the device page if the displayed version is stale. The
   ESP32 node reports the first eight bytes of its build version over CAN.

The node checks the length and CRC-32 of the image read back from flash.
It boots the new image as pending and confirms it after the gateway
answers its CAN echo. If the echo watchdog restarts it before confirmation,
the OTA bootloader can return to the previous slot. A bench node with
watchdog restarts disabled confirms after a grace period instead. If
the node remains unreachable, check the CAN connection and recover an
ESP32 node using its [serial flashing setup](../README.md#building).

For compatible older firmware that does not answer the block handshake,
the gateway falls back to the legacy unacknowledged stream. Verify the
version on each such node after the upload. The web UI sends updates to
multiple nodes of the same type one at a time. The REST device-type
broadcast uses the legacy stream. See the
[block OTA wire format](#firmware-update-in-blocks-otacontrol--otadata) and the
[OTA design notes](../software/gateway-core/docs/adr/0017-block-ota.md).

### Upload over REST and read the result

Send an authenticated `PUT` with the raw `.bin`, its `Content-Length`,
and its CRC-32 as an unsigned decimal query parameter. Use the trusted
certificate for your gateway. The following shell example uses the
gateway's HTTPS address and prompts for the web password.

```sh
base=https://gateway.example.local/api/v1
user=admin
cert=./gateway-ca.pem

gateway_api() {
  curl --fail-with-body --silent --show-error --cacert "$cert" --user "$user" "$@"
}

upload() {
  crc=$(python3 -c 'import binascii, pathlib, sys; print(binascii.crc32(pathlib.Path(sys.argv[1]).read_bytes()))' "$2") || return
  gateway_api --request PUT --header 'Content-Type: application/octet-stream' \
    --data-binary "@$2" "$base/$1?crc32=$crc"
}

# Gateway, then check the version once it has restarted.
upload firmware/gateway ./gateway-hardware.bin
gateway_api "$base/system"

# One node by its hex CAN key (a unique URL-encoded name also works).
upload devices/0x10040a00/firmware ./cancomponents.bin
gateway_api "$base/devices/0x10040a00"
```

A successful response reports `bytes` and `restarting`. The latter is
`true` for a gateway restart. For a node with block OTA, HTTP success means
the node acknowledged the verified image. Check its running `version`
separately after it rejoins. Only one upload runs at a time. While uploading
to a node, poll `GET /api/v1/firmware/job` from another connection for
`state`, `target`, `sent`, `total`, `protocol`, `retries`, and `error`.
The gateway keeps this job status in RAM and clears it on restart.

| Response | What to check |
|---|---|
| `404 not_found` | The addressed node must be in the device list. Run a scan and check its key. |
| `409 busy` | Wait for the current upload to finish. |
| `413 size` | Check that the file is a nonempty application image and fits the target. |
| `422 verify` | The gateway rejected the image length or checksum. Recheck the file and CRC-32. |
| `502 update_failed` | The node update failed. Read `message` in the response and `error` in the job to see whether it was a timeout, a transfer error, or verification. |

See the [REST reference](../software/gateway-core/docs/api-rest.md#firmware)
for the full route list, or [run the OTA tests in QEMU](#test-firmware-updates-with-qemu).

## QEMU simulation

Run the gateway and CAN-node firmware on Linux with one QEMU process per
ESP32. The simulator connects the emulated CAN controllers over SocketCAN
and models button inputs, relay outputs, and I²C and 1-Wire devices. The
gateway's web UI and REST API are available on forwarded localhost ports.

### Prepare the simulator

Use Python 3.11 or newer, the ESP Rust toolchain, `espflash`, and a Linux
kernel with SocketCAN and `vcan`. Build the
[project's QEMU fork](https://github.com/Th3Link/qemu/tree/esp32-sim).
Its `esp32-sim` branch includes the Ethernet MAC, GPIO, remote I²C and
1-Wire models used by the firmware.

On Ubuntu, install the host tools and QEMU build dependencies.

```sh
sudo apt-get install git build-essential pkg-config cmake ninja-build python3 python3-venv \
  libglib2.0-dev libpixman-1-dev libslirp-dev libgcrypt20-dev iproute2
```

From the homeautomation repository root, build QEMU in a separate checkout
and put the resulting binary in `QEMU`.

```sh
git clone --branch esp32-sim https://github.com/Th3Link/qemu.git "$HOME/qemu-esp32-sim"
(
  cd "$HOME/qemu-esp32-sim"
  ./configure --target-list=xtensa-softmmu --enable-gcrypt --enable-slirp \
    --disable-werror --disable-user --disable-docs
  ninja -C build qemu-system-xtensa
)
export QEMU="$HOME/qemu-esp32-sim/build/qemu-system-xtensa"
```

Install the [ESP Rust toolchain](../README.md#building) and install
`espflash` with `cargo install espflash --locked` if it is missing.
Build the two release ELFs. The simulator uses them to prepare the flash
images for each chip.

```sh
source "$HOME/export-esp.sh"
(cd software/gateway-hardware && cargo build --release --locked)
(cd software/cancomponents-hardware && cargo build --release --locked)

cd software/sim
python3 -m venv "$HOME/.venvs/homeautomation-sim"
source "$HOME/.venvs/homeautomation-sim/bin/activate"
python -m pip install pytest
```

### Start a gateway and several nodes

The examples use [`tests/sim.toml`](../software/sim/tests/sim.toml).
It places `gw`, a sensor-equipped switch (`taster`), a relay board
(`relais`), and a PWM node (`pwm`) on the same `vcan-test` bus.
Run these commands from `software/sim`. The `net` command uses
`sudo` to create the virtual interface if it is missing.

```sh
./sim -f tests/sim.toml net
./sim -f tests/sim.toml doctor
./sim -f tests/sim.toml up -d
./sim -f tests/sim.toml status
```

Open `https://127.0.0.1:8543/` for this gateway, or run
`./sim -f tests/sim.toml tui` to see the chips and their events together.
The first status output may show `booting` while the firmware starts.
The `https` field selects the host port forwarded to port 443 on the QEMU
gateway. Inside QEMU, the host network is reachable at `10.0.2.2`.

To build your own installation, copy a topology and add a
`[[node]]` for each simulated board. `gateway = "gw"` puts the node
on that gateway's CAN interface. Give each node a unique name and each
node of the same type on that bus a distinct CAN ID. The
[default topology](../software/sim/sim.toml) shows two gateways with
separate buses and localhost ports. Use the same `-f` path in every
command for a custom topology.

The test topology keeps virtual flash under `tests/.sim/sim/chips/`.
Device settings persist across runs. `./sim -f tests/sim.toml down` stops
the installation. For troubleshooting, read `tests/.sim/sim/up.log` or
the `serial.log` in each chip's directory. Use
`./sim -f tests/sim.toml can --all` to watch protocol traffic, including
the [discovery messages](#can-discovery).

### Control nodes from Python

`sim run` provides a connected `sim` object to the script. This example
presses two simulated buttons and waits for the corresponding CAN
reports from the two nodes.

```sh
./sim -f tests/sim.toml run examples/first_steps.py
```

The script prints one decoded event per node.

```text
taster: button 1 double
relais: button 5 single
```

The [example script](../software/sim/examples/first_steps.py) waits until
the gateway and nodes are ready. It looks up each node by its TOML name,
then pairs each input with the expected CAN frame. The double press on
`taster` matches the payload in the [wire-format example](#decode-a-button-event).

```python
taster = sim["taster"]
taster.press(1, "double")
sim.expect_can("ButtonEvent", chip=taster, data=[1, 4, 0, 0])

relais = sim["relais"]
relais.press(5)
sim.expect_can("ButtonEvent", chip=relais, data=[5, 3, 0, 0])
```

To change a sensor input, call `taster.bme680.set(temperature=27.25)`
and use `sim.expect_can("TemperatureSensor", chip=taster, value=27.25, timeout=40)`.
Python can also connect directly via [`simlib.Sim`](../software/sim/README.md#scripts).

### Verify CAN discovery with pytest

Keep the `vcan-test` interface up. Stop the interactive session so pytest
can start and stop its own supervisor, then run the discovery test.

```sh
./sim -f tests/sim.toml down
python -m pytest tests/test_discovery.py -q
```

The [test](../software/sim/tests/test_discovery.py) sends `discover` to the
gateway's serial console. It waits for the broadcast request, then checks
the name announcement and addressed acknowledgement for each of the three
nodes. The captured frames are the exchange described in [CAN discovery](#can-discovery).

Run `python -m pytest tests` for the whole suite, which also exercises
HTTPS. To run just discovery with fresh flash images, copy the test topology
to an empty directory. The copy keeps the fixture names, CAN interface
and ports, so stop the existing test installation first.

```sh
fresh=$(mktemp -d)
cp tests/sim.toml "$fresh/sim.toml"
python -m pytest tests/test_discovery.py --rootdir=. --sim-topology "$fresh/sim.toml"
```

QEMU can check firmware behavior, CAN messages, and the gateway API.
For physical bus timing and electrical behavior, use hardware. See the
[simulator reference](../software/sim/README.md#how-it-works) for the
emulated peripherals and their limits.

### Test MQTT with two gateways

From `software/sim`, install Mosquitto and its command-line clients,
create the two dedicated CAN interfaces, and run the MQTT tests.

```sh
sudo apt-get install mosquitto mosquitto-clients
./sim -f tests/mqtt.toml net
python -m pytest tests/test_mqtt.py -q
```

The [test topology](../software/sim/tests/mqtt.toml) runs a switch and
a relay node on each of two separate CAN buses. Pytest copies the
topology to a temporary directory, starts Mosquitto on a localhost port,
and boots all six chips with fresh flash images. It checks traffic in
both directions, verifies that a named relay command affects only the
intended bus, and checks retained temperature and button configurations
for both gateways. A Mosquitto restart with persistence disabled clears
its retained store. The test verifies that both gateways recreate the
configurations with the same IDs. On failure, the test output gives the
directory containing broker, CAN, supervisor, and serial logs.

### Test firmware updates with QEMU

Use the [simulator prerequisites](#prepare-the-simulator), then run the
following commands from `software/sim`. The build helper uses the current
source and lockfile to produce two release builds labeled `qemu-v1` and
`qemu-v2`. By default, the builds use `target/qemu-ota` and the script
copies the ELFs and application images into the scratch directory. The
labels fit in a CAN version reply.

```sh
ota_images=$(mktemp -d)
bash tests/build-ota-images.sh "$ota_images"
./sim -f tests/ota.toml net
python -m pytest tests/test_firmware_update.py --ota-images "$ota_images" -q
```

Each test boots a gateway and two switches from `qemu-v1` on fresh
virtual flash. The test gateway uses HTTPS with a test login and its generated
certificate on a forwarded loopback port. The two tests submit an invalid CRC, inspect the API error and
job state, and check that the rejected update leaves `qemu-v1` running.
The valid uploads check the gateway's new version and saved settings
after reboot, and the updated node's rediscovery. The test reads the
nodes' versions from fresh CAN replies to confirm that the target runs
`qemu-v2` and its peer still runs `qemu-v1`. The test output locates the
flash and serial logs if an assertion fails.

After a guest reset, QEMU can lose CAN communication with the target
node. If this happens, pytest first checks the bootloader
log for the new OTA slot, then restarts that QEMU process using the same
flash. It checks discovery and the version replies once CAN is connected
again.

### Run the MQTT demo

Use the same topology to explore the live topics or connect Home
Assistant. From `software/sim`, start a separate broker and a copy of
the topology. Its anonymous listener is bound to the host loopback for
the demo. A Home Assistant instance on the same network namespace can
use `127.0.0.1:18883`. The emulated gateways reach it through
`10.0.2.2:18883`. Stop the demo before running pytest in the same network
namespace, since both use the dedicated MQTT CAN interfaces.

```sh
./sim -f tests/mqtt.toml net
demo=$(mktemp -d)
cp tests/mqtt.toml "$demo/mqtt.toml"
printf 'listener 18883 127.0.0.1\nallow_anonymous true\npersistence false\n' > "$demo/mosquitto.conf"
mosquitto -c "$demo/mosquitto.conf" -v > "$demo/mosquitto.log" 2>&1 &
broker_pid=$!
./sim -f "$demo/mqtt.toml" up -d
./sim -f "$demo/mqtt.toml" status
```

Try the [MQTT examples](#topics-and-device-addresses) once the broker
shows both gateways connected. When finished, stop the demo from
the same shell.

```sh
./sim -f "$demo/mqtt.toml" down
kill "$broker_pid"
```

## Firmware memory

Open **Memory report** in the handbook sidebar for the ESP32 memory map,
Debug and Release ELF history, and the latest gateway workload measurements.
The application binary is compared with the smallest app slot in the board's
partition table. The ELF breaks RAM down by physical bank.

| Bank | ESP32 capacity | Gateway placement |
|---|---:|---|
| SRAM0 | 192 KiB | 64 KiB flash cache, plus the vectors and IRAM code in the remaining 128 KiB |
| SRAM1 | 128 KiB | Two MQTT connections, five HTTPS socket buffers, 4 KiB of OTA flash scratch, the static DHCPv6 client and its packet buffers, a shared 14 KiB workspace, a 16 KiB general heap and a 52 KiB MbedTLS pool, alongside ROM data. The instruction address range mirrors the data range. |
| SRAM2 | 200 KiB | 8 KiB reserved by the ROM and 192 KiB for primary DRAM. The automation task holds its reload snapshot here. MQTT, EST and NTS share a single cached copy of the installed trust anchors (up to 6 KiB). The linker gives the main stack the space left after statics. |
| RTC FAST | 8 KiB | CAN log frame queue, MQTT log batch, one slot used for either certificate status or MQTT certificate capture |
| RTC SLOW | 8 KiB | Node log text for the web API |

The [gateway RAM notes](../software/gateway-core/docs/ram.md) identify the
linker ranges and their contents. RTC memory is accounted for separately.

### Read the runtime measurements

`GET /api/v1/system` includes `boot_id` and uptime, heap size and usage,
`heap_peak_used` and `heap_min_free`, and `stack_size`, `stack_peak_used`
and `stack_untouched`. All memory values are in bytes. The allocator
records peak usage as allocations happen, so a buffer that has already
been freed can still affect the peak. The stack peak is the linker budget
minus the remaining stack paint. This estimate includes the unpainted
guard and startup area. Zero untouched bytes mean no reserve was observed.
The `crypto_pool` object reports MbedTLS capacity, current use, peak use
and failed allocation count. Its peak and the general heap peak can occur
at different times.

A profiling build emits UART samples every two seconds. This keeps the
measurement available during MQTT or CAN activity even if a web request
fails. The `idle-rest` scenario also samples over REST for ten seconds to
include the cost of querying `/system`. The report keeps samples for each
boot separate. A panic, missing samples, or an incomplete workload is
visible in the result, and a failed workload can still have measurements
from before the failure.

### Reproduce the gateway profiles

Use the [QEMU prerequisites](#prepare-the-simulator), Mosquitto with its
command-line clients, and OpenSSL. From `software/sim`, use the QEMU build
from the [revision pinned in CI](../.github/workflows/simulation.yml).
Set `QEMU` to the executable in the QEMU checkout's `build/` directory.

```sh
export CARGO_TARGET_DIR="$PWD/../../target/qemu-memory"
qemu_revision=$(git -C "$(dirname "$QEMU")/.." rev-parse HEAD)
ota_images=$(mktemp -d)
GATEWAY_FEATURES=memory-profile bash tests/build-ota-images.sh "$ota_images"
./sim -f tests/sim.toml net
./sim -f tests/mqtt.toml net
./sim -f tests/ota.toml net
python memory_profile.py qemu --qemu-revision "$qemu_revision" \
  --ota-images "$ota_images" --output "$(mktemp -d)"
```

The runner uses a fresh virtual installation for each repeat and saves
`runtime.json` with the firmware SHA, build profile, QEMU revision, broker
version, topology hashes, and results per boot. The output directories also
hold the generated topologies and logs. Use `--scenario` to select a case
and `--runs 1` for a single measurement.

| Scenario | Measured workload |
|---|---|
| `idle`, `idle-rest`, `heap-probe` | Gateway idle over UART, idle with REST requests, and an 8 KiB allocation freed before the next sample |
| `sntp-dns` | UART sample while the time server’s DNS lookup waits for a reply on an isolated guest LAN. The general heap must be empty. |
| `can-mqtt`, `ha-reconnect` | Two gateways with four nodes and Mosquitto, then MQTT discovery republished after a broker restart |
| `dual-mqtt`, `dual-mqtt-tls` | Commands from a second broker while five HTTPS clients download the UI, including a retained 1,800-byte payload and a TLS certificate larger than 4 KiB |
| `mqtt-pinned` | Connect to Mosquitto with a pinned public key, renew its certificate with the same key, then check that a different key is rejected |
| `https-est` | Enroll and renew over mutual TLS with built-in and installed trust anchors, a client identity chain and chunked PKCS#7 replies |
| `https-cert-rotation` | Replace the active HTTPS certificate, reconnect and check the TLS pool |
| `https-trust-import` | Install a six-certificate CA bundle as PEM and DER, read its fingerprint and count, reject invalid uploads and remove it |
| `https-identity-import` | Install an EST client certificate chain with its private key, check the leaf fingerprint, reject a key-only upload and remove the identity |
| `https-automation`, `combined` | Automation edits over HTTPS, including the button-save stack watermark and output state across configuration reloads, and an HTTPS request alongside active MQTT nodes |
| `gateway-ota`, `can-ota` | The firmware update tests, including CRC rejection and the successful transfer |
| `mqtt-tls` | Two gateways using TLS 1.3 with a local test broker and a generated certificate, with certificate verification disabled in the isolated test |

The two certificate import scenarios start on separate fresh gateways. Each
`pytest.log` records the general heap high-water mark immediately after the
first upload. The peak in `runtime.json` covers the whole scenario, including
the subsequent read of the certificate details at `GET /api/v1/tls`.

The EST test starts a local SNTP peer before boot so the gateway can validate certificate dates. In an isolated test network, make loopback UDP port 123 available to the test user. CI redirects requests from QEMU to the unprivileged port set in `SIM_SNTP_PORT`. Without a usable port, pytest skips this test.

For a physical gateway, use the same API sampler with the device's trusted
certificate. The output is labeled `hardware` and includes the running
firmware version and sample window.

```sh
read -rsp 'Gateway password: ' GATEWAY_PASSWORD
export GATEWAY_PASSWORD
python memory_profile.py hardware https://gateway.example.local --user admin \
  --cafile gateway-ca.pem --seconds 30 --output hardware-memory.json
unset GATEWAY_PASSWORD
```

The [Simulation workflow](../.github/workflows/simulation.yml) runs two
repeats on schedule or on manual dispatch. Pull requests run the two-gateway
MQTT test, the dual-broker TLS load test and the automation stack regression. The memory page links to the latest scheduled or manually
started run on `main`, including failures and runs that produced no artifact.
Compare reserves across repeated runs of the same build and workload before
using the measurements as thresholds.

## Rust API reference

| Component | Core | ESP32 |
|---|---|---|
| Gateway | [gateway-core](gateway-core/gateway_core/index.html) | [gateway-hardware](gateway-hardware/gateway_hardware/index.html) |
| CAN nodes | [cancomponents-core](cancomponents-core/cancomponents_core/index.html) | [cancomponents](cancomponents-hardware/cancomponents/index.html) |
| Shared code | [common-core](common-core/common_core/index.html) | [common-hardware](common-hardware/common_hardware/index.html) |

[Gateway REST API](../software/gateway-core/docs/api-rest.md) ·
[Architecture decisions](adr/index.md)
