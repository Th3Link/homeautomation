# Simulation

Runs the gateway and node firmware, unchanged, in QEMU. Several gateways
each get their own virtual CAN bus with nodes on it. You press buttons, walk
past a PIR, change the light, and watch relays switch and frames go by.
Here a double press runs an automation that switches relay 3:

```console
$ ./sim up
  12 chips  from software/sim/sim.toml
  gw-eg          gateway  vcan0  https://127.0.0.1:8443
  taster-flur    button   vcan0  id 10  buttons 0,1,2,3  PIR  200 lx
  relais-keller  relay    vcan0  id 128  PIR  40 lx  relays (relais)
  …
15:45:32  gw-eg          network up 10.0.2.15  web UI https://127.0.0.1:8443
15:45:32  taster-flur    discovered by the gateway
15:45:51  taster-flur    👆 button 0 double
15:45:51  taster-flur    button 0 double
15:45:51  relais-keller  ← relay 3 → on
15:45:51  relais-keller  ⚡ relay 3 on
```

In a second terminal:

```sh
./sim status                     # what runs, IPs, relays, light
./sim press taster-flur 0 double # single (default), double, triple, hold [-t secs]
./sim motion taster-flur 30      # the PIR sees someone for 30 s
./sim lux taster-flur 5          # dusk
./sim console gw-eg              # the serial console; Enter wakes it, Ctrl-] leaves
./sim web gw-eg                  # open the web UI
./sim can                        # the CAN traffic, decoded and named (--all: everything)
./sim log taster-flur -f         # a chip's serial log
./sim restart taster-flur        # power cycle
./sim down                       # stop everything (or Ctrl-C in the up terminal)
```

Chips can be named by a unique prefix (`./sim press taster-f 0`). Every
command has `--help`.

## Setup

1. **QEMU** with the models this needs, branch `esp32-sim` of
   [Th3Link/qemu](https://github.com/Th3Link/qemu/tree/esp32-sim), on top of
   Espressif's QEMU. Plain Espressif QEMU boots neither firmware: it has no
   interrupt status registers, no GPIO, no external I2C, no Ethernet MAC
   and no analog I2C for the APLL.

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
2. **CAN interfaces**, one per gateway, as root once per boot:

   ```sh
   sudo modprobe vcan
   sudo ip link add vcan0 type vcan && sudo ip link set vcan0 up
   sudo ip link add vcan1 type vcan && sudo ip link set vcan1 up
   ```

3. **Firmware**: `cargo build --release` in `cancomponents-hardware` and
   `gateway-hardware`. You also need **espflash** and Python 3.11 or newer
   (no packages).

`./sim doctor` checks all of this.

## Topology

[`sim.toml`](sim.toml) describes the installation: gateways, and nodes with
type, CAN id, the gateway whose bus they are on, hardware revision and
extension. To try your own, copy it (`./sim -f mine.toml up`, or set
`SIM_TOPOLOGY`). The comment at its top lists every field.

| Node type | Board | Buttons | Relays | Extension |
|---|---|---|---|---|
| `button` | light switch, hwrev 1 or 2 | 0-3, 4-7 with `extension = "button"` | — | `button` or `sensors` |
| `relay` | relay board, hwrev 1 or 2 | 4-7 with a button extension | 12 (PCA9554 at 0x26/0x27) | `button` or `sensors` |
| `shutter` | relay board in shutter mode, hwrev 1 | as relay | relay pairs | as relay |
| `heating` | relay board with the heating controller | as relay | 12 | as relay |

`extension = "sensors"` adds the PIR and a VEML7700 light sensor.

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
  hwrev, extension, relay mode, and any `console` commands from the
  topology.
- **A new release build** replaces the apps and keeps the data partitions
  (`nvs`, `storage`), as an update in the field would.
- **Changed settings** in the topology are typed in again.
- `./sim reset <name>` (or `--all`) makes a chip factory-fresh.

## How it works

- **QEMU per chip:** each chip runs in its own QEMU (`-machine esp32`), and
  `sim up` supervises them.
- **CAN:** the chip's TWAI is on the host interface of its bus, so `candump`
  and `cansend` work too.
- **GPIO:** a socket per chip carries `pin <n> <0|1>` lines to QEMU and
  reports outputs back as `out <n> <0|1>`. Buttons are active low; the PIR
  is active high.
- **I2C:** QEMU's `i2c-remote` asks `sim up` about every transfer
  (`simlib/i2c.py`, which documents the protocol). `sim up` plays the relay
  expanders and the VEML7700. A new sensor is a class there.
- **Ethernet:** the gateway's Ethernet runs on QEMU's user network. DHCP
  gives it 10.0.2.15. The host is 10.0.2.2, for example for an MQTT broker.
  The web UI is forwarded to the `https`/`http` ports.
- **Logs and control:** serial logs go to `.sim/<topology>/chips/<name>/serial.log`.
  The other commands talk to `sim up` over a Unix socket in
  `$XDG_RUNTIME_DIR`.

## Limits

- **Not modelled:** the BME680, SCD30, DS18B20/1-Wire, the night light
  (PCF8574A), PWM/LEDC and the ADC. The STM32 lamp controllers aren't
  modelled either; their frames show up in `sim can` and in the `up` log.
- **Timing:** time runs as fast as QEMU emulates. It is close to real time
  but not exact, so timing tests belong on the bench.
- **CPU:** a gateway uses about one CPU core and a node about 5 % of one.
