# Simulation

Runs the gateway and node firmware, unchanged, in QEMU. Several gateways
each get their own virtual CAN bus with nodes on it. Around the chips the
simulation plays the hardware:

- buttons and the PIR;
- the relays;
- the sensor extension: light, BME680 (or BME280), SCD30 and the night light;
- PCA9685 PWM boards;
- DS18B20s on the 1-wire header.

You press buttons, set temperatures, unplug a sensor, and watch relays
switch and frames go by. Do it from the command line, in a TUI, from a
Python script, or in pytest.

Here a double press runs an automation that switches relay 3:

```console
$ ./sim up
  12 chips  from software/sim/sim.toml
  gw-eg          gateway  vcan0  https://127.0.0.1:8443
  taster-flur    button   vcan0  id 10  buttons 0,1,2,3  PIR  veml7700 bme680 scd30 nightlight
  relais-keller  relay    vcan0  id 128  PIR  veml7700 bme680 nightlight ds18b20-0  relays own relais (relais)
  …
15:45:32  gw-eg          network up 10.0.2.15  web UI https://127.0.0.1:8443
15:45:32  taster-flur    discovered by the gateway
15:45:34  taster-flur    temperature 21.5 °C  bme680@77
15:45:51  taster-flur    👆 button 0 double
15:45:51  taster-flur    button 0 double
15:45:51  relais-keller  ← relay 3 → on
15:45:51  relais-keller  ⚡ relay 3 on
```

In a second terminal:

```sh
./sim tui                        # everything in one screen (below)
./sim status                     # what runs, IPs, relays, sensor values
./sim press taster-flur 0 double # single (default), double, triple, hold [-t secs]
./sim motion taster-flur 30      # the PIR sees someone for 30 s
./sim set taster-flur bme680 temperature=25 humidity=60
./sim lux taster-flur 5          # dusk (= set taster-flur veml7700 lux=5)
./sim get relais-keller          # its parts, their values, what can be set
./sim set relais-og ds18b20-1 plugged=0   # a DS18B20 drops off the bus
./sim i2c taster-flur            # every I2C transaction, live
./sim console gw-eg              # the serial console; Enter wakes it, Ctrl-] leaves
./sim web gw-eg                  # open the web UI
./sim can                        # the CAN traffic, decoded and named (--all: everything)
./sim events -k device -k relay  # follow what happens, filtered
./sim log taster-flur -f         # a chip's serial log
./sim restart taster-flur        # power cycle
./sim down                       # stop everything (or Ctrl-C in the up terminal)
```

Chips and parts can be named by a unique prefix
(`./sim set taster-f bme temperature=20`). Every command has `--help`.
`./sim up -d` runs in the background, with its output in
`.sim/<topology>/up.log`.

## TUI

`./sim tui` shows the running simulation in one screen. If it isn't
running yet, the TUI starts it and offers to stop it when you leave.

- **Left:** the chips.
- **Right:** the selected chip: its buttons, the PIR, the relays and every
  part with its values.
- **Below:** the events.

| Key | Does |
|---|---|
| `↑` `↓` | choose a chip |
| `0`-`7` | press that button; `d`, `t`, `h` first for double, triple, hold |
| `m` / `M` | motion for 10 s / 60 s |
| `←` `→` `Tab` | choose a value (temperature, lux, CO2, …) |
| `+` `-` / `Enter` | change it by one step / type a value |
| `c` | the chip's console (Ctrl-] comes back) |
| `r` | restart the chip |
| `f` | events of all chips or the selected one |
| `s` `i` `a` | also show serial lines, I2C transactions, CAN housekeeping |
| `:` | a command: `press 0 double`, `set bme680 temperature=25`, `motion 30`, `lux 5`, `pin 4 0`, `i2c 0x77 0xd0 1`, `restart` |
| `?` / `q` | all keys / leave (the simulation keeps running) |

## Parts

What a node has depends on its type and extension in the topology. Every
value can be set while it runs: `./sim set`, the TUI, or a script.

| Part | Where | Values | What the firmware does with it |
|---|---|---|---|
| `veml7700` | I2C1 0x10 | `lux` | `AmbientLightSensor` on a 10 % change |
| `bme680` | I2C1 0x77 | `temperature`, `humidity`, `pressure`, `gas` (kΩ) | `TemperatureSensor`, `HumiditySensor`, `PressureSensor`, `GasResistance`, and after its warm-up the air quality |
| `bme280` | I2C1 0x76 | `temperature`, `humidity`, `pressure` | nothing: the firmware has no driver (it logs "no BME680") |
| `scd30` | I2C1 0x61 | `co2`, `temperature`, `humidity` | `Co2Sensor` |
| `nightlight` | I2C1 0x38 (PCF8574A) | — (shows the level, 0-3) | follows `Nightlight` |
| `pwm-1` … `pwm-16` | I2C1 0x40… (PCA9685) | — (shows each channel's duty) | follows `LampGroup` with bank 1-16 |
| `ds18b20-0` … | 1-wire, GPIO 4 | `temperature`, `plugged` (0/1) | `TemperatureSensor` per sensor; `DeviceError` when one is lost |
| relays, bank 0 | I2C0 0x26/0x27 (PCA9534) | — | the node's own board (`./sim status`) |
| relays, banks 1-4 | I2C1 0x20/0x21 … 0x26/0x27 (PCA9534) | — | relay boards without a µC on the extension bus, see below |

**Relay boards:** a `Relais` command's last byte, the bank, picks the
board. Bank 0 is the node's own (relay, shutter and heating nodes); banks
1-4 are boards without a µC on the extension bus, their JP5/JP6 bridged
and their A1/A2 solder bridges set (bank 4 is the factory setting). They
sit next to the sensors or the PWM boards, on any node with an I2C
extension. A board is the relay module (`relais`, 12 relays) or the SSR
module (`ssr`, 16 outputs):

```toml
[[node]]
name = "taster-flur"
type = "button"
extension = "sensors"               # or pwm, or relais (only the boards)
relay_boards = ["relais", "ssr"]    # banks 1 and 2; { 4 = "relais" } for one bank
```

`./sim status`, `get` and the TUI show each board (`bank 1 ○○○…`), relay
events name the bank (`⚡ bank 2 relay 15 on`), and in a script
`chip.bank(2)` is that board's relays. On the node's console,
`relais 15 on 2` switches relay 15 of bank 2 and `show` lists the boards
it found.

The sensor frames carry the value as the gateway sees it. `./sim can`
decodes them, for example `temperature 21.5 °C  bme680@77`. The BME680
model inverts the `bme68x` crate's compensation, with that crate's
reference calibration. A set value therefore arrives as that value, to
the frame's 1/16 resolution.

## Raw I2C

The I2C buses of a node end in `sim up`: QEMU forwards every transfer of
the firmware to it (`simlib/i2c.py` describes the line protocol). So you
can watch the bus and act as a second master:

```console
$ ./sim i2c taster-flur                  # live, every transaction (Ctrl-C)
19:02:11  taster-flur  i2c1 10 W 04 R 2e 03
19:02:12  taster-flur  i2c1 77 W 71 10 72 01 74 25
19:02:12  taster-flur  i2c1 77 W 1d R 80 01 49 b8 90 7d 90 60 5c 06 00 00 00 3a 37 00 00
$ ./sim i2c taster-flur scan             # -b 0 for the node's own relay board
  0x10  veml7700
  0x38  nightlight
  0x61  scd30
  0x77  bme680
$ ./sim i2c taster-flur read 0x77 0xd0   # register 0xd0 (the chip id), 1 byte
61
$ ./sim i2c taster-flur read 0x61 0xd1 3 # reads after writing 0xd1: SCD30 firmware version
03 42 f3
$ ./sim i2c relais-keller write 0x26 1 0x08 -b 0   # PCA9534 output register: relay 0 on
$ ./sim i2c taster-flur dump 0x77        # all 256 registers
```

`read <addr> <reg> <n>` writes the register, then reads `n` bytes after a
repeated start. `write <addr> <bytes…>` is one plain write. Your transfers
wait for the firmware's: they never land inside one of its transactions.
`./sim up --i2c` prints every transaction too, and so does `i` in the TUI.

## Scripts

`simlib.Sim` drives a running simulation from Python: everything the
commands do, and waiting for what follows. `./sim run script.py` runs a
script with `sim` already connected:

```python
flur = sim["taster-flur"]
flur.press(0, "double")
sim.expect_can("ButtonEvent", chip=flur, data=[0, 4])    # button 0, double

flur.bme680.set(temperature=25.5)
frame = sim.expect_can("TemperatureSensor", chip=flur, sensor="bme680@77", value=25.5, timeout=40)

flur.veml7700.ramp("lux", 5, secs=30)                    # dusk over 30 s
sim.send_can("Nightlight", to=flur, data=[2])            # as the gateway would
sim.expect("night light", chip=flur, kind="device")
assert flur.nightlight.level == 2

relais = sim["relais-og"]
relais.ds18b20_1.set(plugged=0)
sim.expect("LOST", chip=relais, kind="serial", timeout=90)
print(relais.console("relais 3 on"))                     # the console's answer
print(flur.i2c(0x77, b"\xd0", 1))                        # raw I2C: b'a'
```

- `sim[name]` is a chip; `chip.<part>` is a part (`ds18b20-0` as
  `ds18b20_0`). A part has `set(**values)`, `get()`, its values as
  attributes, and `ramp(key, to, secs)`.
- `chip.press`, `motion`, `pin`, `restart`, `relays` (the own board's),
  `bank(n)` (relay bank `n`), `console(command)` and
  `i2c(addr, write, read)` act on the chip.
- `sim.expect(pattern, chip=, kind=, timeout=, where=)` waits for an
  event. `sim.expect_can(message, chip=, data=, value=, sensor=)` waits
  for a frame. `sim.no_event(...)` checks that none comes. All of them
  look at what happens after the script's last action, so nothing in
  between is missed. A miss raises `Timeout`, an `AssertionError`.
- `sim.send_can(message, to=chip, data=...)` sends a frame on the
  chip's bus as the gateway would.
- Events are dicts: `kind` (`info`, `action`, `can`, `relay`, `device`,
  `serial`, `i2c`, `mqtt`), `chip`, `text`, and the fields of their kind
  (`simlib/supervisor.py`).

`examples/` has a few: `dusk.py` (light, motion, humidity),
`heating.py` (a cooling buffer tank) and `tmp102.py` (a part of your own,
see below). Without `./sim run`, a script imports it itself:
`sys.path.insert(0, "software/sim"); from simlib import Sim`.

## Tests

`tests/` runs pytest against its own installation, `tests/sim.toml`: a
gateway, a light switch with every sensor, a relay board with two
DS18B20s and buttons, and a PWM node. It runs on its own bus,
`vcan-test`, so it doesn't meet a `./sim up` you have running.

```sh
./sim -f tests/sim.toml net            # once per boot: the vcan-test bus
python -m pytest tests                 # starts the simulation, stops it at the end
python -m pytest tests -k onewire      # some
python -m pytest tests --sim-keep      # leave it running; the next run reuses it
```

The fixtures are `sim` (a `Sim` for the session) and the chips by name
(`taster`, `relais`, `pwm`). A test is a script with asserts:

```python
def test_bme680_temperature(sim, taster):
    taster.bme680.set(temperature=27.25)
    sim.expect_can("TemperatureSensor", chip=taster, sensor="bme680@77", value=27.25)
```

The first run builds and configures the images (about a minute). The
whole suite takes a few minutes: the sensors report every 10 s and a
DS18B20 counts as lost after three missed cycles. Tests for another
installation: `--sim-topology mine.toml`.

## Setup

1. **QEMU** with the models this needs, branch `esp32-sim` of
   [Th3Link/qemu](https://github.com/Th3Link/qemu/tree/esp32-sim), on top of
   Espressif's QEMU. Plain Espressif QEMU boots neither firmware: it has no
   interrupt status registers, no GPIO, no 1-Wire, no external I2C, no
   Ethernet MAC and no analog I2C for the APLL.

   ```sh
   git clone -b esp32-sim https://github.com/Th3Link/qemu.git && cd qemu
   ./configure --target-list=xtensa-softmmu --enable-gcrypt --enable-slirp \
     --disable-werror --disable-user --disable-docs
   ninja -C build
   ```

   Put `build/qemu-system-xtensa` on `PATH`, or set `QEMU`.

   `.github/workflows/qemu-sync.yml` rebases `esp32-sim` onto Espressif's
   `esp-develop` every Monday. It builds the result, checks that the models
   are there and force-pushes. If that fails, the branch stays as it is and
   an issue opens here. A local checkout follows with
   `git fetch && git reset --hard origin/esp32-sim`.
2. **CAN interfaces**, one per gateway, once per boot. `./sim up` creates
   missing ones with sudo and asks for your password. `./sim net` does
   only that, and `./sim net down` removes them. By hand:

   ```sh
   sudo modprobe vcan
   sudo ip link add vcan0 type vcan && sudo ip link set vcan0 up
   ```

3. **Firmware**: `cargo build --release` in `cancomponents-hardware` and
   `gateway-hardware`. You also need **espflash** and Python 3.11 or newer
   (no packages; pytest only for the tests).

`./sim doctor` checks all of this.

## Topology

[`sim.toml`](sim.toml) describes the installation: gateways, and nodes with
type, CAN id, the gateway whose bus they are on, hardware revision,
extension and parts. To try your own, copy it (`./sim -f mine.toml up`, or
set `SIM_TOPOLOGY`). The comment at its top lists every field.

| Node type | Board | Buttons | Relays | Extension |
|---|---|---|---|---|
| `button` | light switch, hwrev 1 or 2 | 0-3, 4-7 with `extension = "button"` | banks 1-4 with `relay_boards` | `button`, `sensors`, `pwm` or `relais` |
| `relay` | relay board, hwrev 1 or 2 | 4-7 with a button extension | 12 (PCA9534 at 0x26/0x27), 16 with `relay_board = "ssr"`; plus banks 1-4 | as button; DS18B20s on the 1-wire header |
| `shutter` | relay board in shutter mode, hwrev 1 | as relay | relay pairs | as button |
| `heating` | relay board with the heating controller | as relay | 12 | as relay |

```toml
[[node]]
name = "taster-flur"
type = "button"
id = 10
gateway = "gw-eg"
extension = "sensors"
sensors = ["veml7700", "bme680", "scd30", "nightlight"]  # the default without scd30
lux = 120

[node.bme680]            # start values, per part
temperature = 22.5
humidity = 55

[[node]]
name = "heizung"
type = "heating"
id = 1
gateway = "gw-og"
ds18b20 = [55.0, { temperature = 38.5, name = "vorlauf" }]
```

**Parts of your own:** a class like those in `simlib/devices.py`, in a
file next to the topology, on either I2C bus of a node. It gets
`./sim set`/`get`, the TUI and raw I2C like the built-in ones.
[`examples/tmp102.py`](examples/tmp102.py) is one.

```toml
[[node.i2c]]
bus = 1
addr = 0x48
model = "examples/tmp102.py:Tmp102"
temperature = 22.5
```

**MQTT:** start a broker on the host, for example
`mosquitto -p 1883 -v`; mosquitto 2 only accepts other hosts with a
listener and `allow_anonymous true` in a config file. The gateways reach it
as 10.0.2.2. Give each gateway the broker in its `console` list. The next
`./sim up` types the commands in, as it does with every changed setting:

```toml
console = ["mqtt-uri mqtt://10.0.2.2:1883", "mqtt-enabled on"]
```

## Images

Each chip has its own flash image under `.sim/<topology>/chips/<name>/`.
That keeps its configuration, the gateway's settings and automations across
runs, like a real flash. `./sim up` builds what is missing:

- **A new chip** gets the release build, a MAC address of its own (in its
  eFuse image), and its settings typed into its console: device type, id,
  hwrev, extension, relay mode and boards, and any `console` commands from the
  topology.
- **A new release build** replaces the apps and keeps the data partitions
  (`nvs`, `storage`), as an update in the field would.
- **Changed settings** in the topology are typed in again.
- `./sim reset <name>` (or `--all`) makes a chip factory-fresh.

The parts' values are not kept: each `./sim up` starts from the topology.

## How it works

- **QEMU per chip:** each chip runs in its own QEMU (`-machine esp32`), and
  `sim up` supervises them.
- **CAN:** the chip's TWAI is on the host interface of its bus, so `candump`
  and `cansend` work too.
- **GPIO:** a socket per chip carries `pin <n> <0|1>` lines to QEMU and
  reports outputs back as `out <n> <0|1>`. Buttons are active low; the PIR
  is active high.
- **I2C:** QEMU's `i2c-remote` asks `sim up` about every transfer.
  `simlib/devices.py` plays the parts, one class each.
- **1-Wire:** the DS18B20s live in QEMU's GPIO model (`onewire_ds18b20.c`),
  because a slot is a few microseconds and the firmware bit-bangs it.
  `sim up` adds, changes and unplugs them over the GPIO socket (`ow …`
  lines). A node with DS18B20s counts time in instructions (`-icount`),
  so its slots don't stretch when the host is busy. That clock runs a
  little slow: a 3 s hold looks like 2.6 s on that node.
- **Ethernet:** the gateway's Ethernet runs on QEMU's user network. DHCP
  gives it 10.0.2.15. The host is 10.0.2.2, for example for an MQTT broker.
  The web UI is forwarded to the `https`/`http` ports.
- **Events and control:** everything that happens is an event
  (`simlib/supervisor.py`). The other commands, the TUI and scripts talk
  to `sim up` over a Unix socket in `$XDG_RUNTIME_DIR` and can follow the
  events. Serial logs go to `.sim/<topology>/chips/<name>/serial.log`.

## Limits

- **Not modelled:** DHT22 and LEDC (only the old C++ firmware used them;
  the Rust firmware has no driver), the ADC, and the STM32 lamp
  controllers. The controllers' frames show up in `sim can` and the `up`
  log. The BME680's air quality is only as good as the gas values you set.
- **Timing:** time runs as fast as QEMU emulates. It is close to real time
  but not exact, so timing tests belong on the bench.
- **CPU:** a gateway uses about one CPU core and a node about 5 % of one.
