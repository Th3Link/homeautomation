<p class="lead">
Wire switches, sensors and actuators into a shared CAN bus. An ESP32
gateway connects the bus to your MQTT broker over Ethernet.
</p>

<ol class="system-path" aria-label="System topology">
  <li><a href="#hardware">CAN nodes</a><span>Switches, sensors, relays</span></li>
  <li><a href="#gateway-quickstart">Gateway</a><span>CAN ↔ Ethernet</span></li>
  <li><a href="#check-delivery-over-mqtt">MQTT broker</a><span>Feeds Home Assistant or openHAB</span></li>
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
[Home Assistant discovery design](../software/gateway-core/docs/adr/0022-home-assistant-discovery.md)

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
The `http` and `https` fields are host ports forwarded into that QEMU
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

## Rust API reference

| Component | Core | ESP32 |
|---|---|---|
| Gateway | [gateway-core](gateway-core/gateway_core/index.html) | [gateway-hardware](gateway-hardware/gateway_hardware/index.html) |
| CAN nodes | [cancomponents-core](cancomponents-core/cancomponents_core/index.html) | [cancomponents](cancomponents-hardware/cancomponents/index.html) |
| Shared code | [common-core](common-core/common_core/index.html) | [common-hardware](common-hardware/common_hardware/index.html) |

[Gateway REST API](../software/gateway-core/docs/api-rest.md) ·
[Architecture decisions](adr/index.md)
