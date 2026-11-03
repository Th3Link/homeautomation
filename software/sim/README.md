# Simulation

Runs the gateway and the node firmware, unchanged, in QEMU. Every chip's CAN
controller sits on a Linux CAN interface (`vcan0`), so several nodes and a
gateway talk to each other as on a real bus. `candump` / `cansend` watch and
inject frames. Outside the chip:

- **GPIO**: a TCP socket per node. Drive inputs (buttons, PIR) and see
  outputs.
- **I2C**: devices modelled in Python (`i2c_models.py`): the PCA9554
  expanders of the relay board and the VEML7700 light sensor.
- **Ethernet** (gateway): QEMU's user network with DHCP. The web UI is
  forwarded to the host.

## Prerequisites

- **QEMU**: Espressif's QEMU with the models this needs, branch `esp32-sim`
  of [Th3Link/qemu](https://github.com/Th3Link/qemu/tree/esp32-sim), on top
  of espressif/qemu `febae18`. Upstream Espressif QEMU boots neither firmware: it has no
  interrupt status registers, no GPIO, no external I2C, no Ethernet MAC and
  no analog I2C for the APLL.

  ```sh
  git clone -b esp32-sim https://github.com/Th3Link/qemu.git && cd qemu
  ./configure --target-list=xtensa-softmmu --enable-gcrypt --enable-slirp \
  --disable-werror --disable-user --disable-docs
  ninja -C build
  ```

  The scripts take `qemu-system-xtensa` from `PATH`, or from `QEMU=…`.

  `.github/workflows/qemu-sync.yml` rebases `esp32-sim` onto Espressif's
  `esp-develop` every Monday. It builds the result, checks that the models
  are there and force-pushes. If the rebase or the build fails, the branch
  stays as it is and an issue opens here. A local checkout then follows
  with `git fetch && git reset --hard origin/esp32-sim`. For a hand rebase,
  run `git rebase` onto espressif's `esp-develop` and push with
  `--force-with-lease`.
- **`vcan0`** (as root, once per boot):

  ```sh
  modprobe vcan && ip link add vcan0 type vcan && ip link set vcan0 up
  ```

- **espflash** and a release build of the firmware:
  `cargo build --release` in `cancomponents-hardware` or `gateway-hardware`.
- **can-utils** for `candump`/`cansend`, and Python 3 for the I2C models.

## Images

Each chip boots from its own 4 MB (node) or 16 MB (gateway) flash image.
Each image keeps that chip's configuration across runs, like a real flash.

```sh
./make-image.sh node node.bin           # from target/…/release/cancomponents
./make-image.sh gateway                 # gateway.bin
cp node.bin button.bin
./configure-node.sh button.bin "device-type 4" "hwrev 1" "device-id 77"
cp node.bin relay.bin
./configure-node.sh relay.bin "device-type 5" "hwrev 2" "device-id 128" \
  "relais-mode relais" "extension-mode sensors"
```

`configure-node.sh` boots the image once and types the commands into the
console. Any console command works there.

`efuse.bin` is created on first use (`make-efuse.py`). It sets chip
revision 3.0, which the bootloader requires.

## Running

One terminal per chip. Ctrl-A X quits; Ctrl-A C switches between the
console and the QEMU monitor.

```sh
./run-gateway.sh                        # https://127.0.0.1:8443
./run-node.sh button.bin                # GPIO on port 5555
candump vcan0
```

A button press on the button node (hwrev 1, button 0 is GPIO 33, active
low):

```sh
printf 'pin 33 0\npin 33 1\n' | nc -q1 127.0.0.1 5555
```

The relay node with its I2C devices:

```sh
./i2c_models.py --bus 5600=pca9554:26,pca9554:27 --bus 5601=veml7700:10
GPIO_PORT=5556 I2C_PORTS="5600 5601" ./run-node.sh relay.bin
```

`i2c_models.py` prints every change of the expanders' outputs (the relays).
Add `-v` to see every I2C transfer.

Several nodes need different `GPIO_PORT`s and different model ports. From
inside, the gateway reaches the host as `10.0.2.2`, for example an MQTT
broker there.

### Environment

| Variable | Script | Default | Meaning |
|---|---|---|---|
| `QEMU` | all | `qemu-system-xtensa` | QEMU binary with the `esp32-sim` models |
| `CAN_IF` | run-* | `vcan0` | host CAN interface |
| `GPIO_PORT` | run-node | `5555` | TCP port for the pins |
| `I2C_PORTS` | run-node | none | model ports for I2C0 and (optionally) I2C1 |
| `HTTPS_PORT`, `HTTP_PORT` | run-gateway | `8443`, `8080` | host ports for the web UI |
| `BOOT_WAIT` | configure-node | `10` | seconds until the console listens |
| `CARGO_TARGET_DIR` | make-image | `../../target` | where the release builds are |

## Protocols

**GPIO** (`esp32.gpio`, one TCP connection, lines):

| Direction | Line | Meaning |
|---|---|---|
| to QEMU | `pin <n> <0\|1>` | drive pin n from outside (inputs; interrupts fire as configured) |
| from QEMU | `out <n> <0\|1>` | pin n is an output and changed |

**I2C** (`i2c-remote`, one TCP connection per bus, QEMU is the client,
hex bytes):

| QEMU sends | Model answers | Meaning |
|---|---|---|
| `Q aa` | `A` / `N` | is there a device at address aa? |
| `S aa W` / `S aa R` | `A` / `N` | start a write / read transfer |
| `W hh` | `A` / `N` | byte written |
| `R` | `hh` | byte read |
| `P` | — | stop |

A new sensor is a class in `i2c_models.py` with `start(write)`, `write(b)`
and `read()`, registered in `MODELS`.

## Limits

- The node's ADC, LEDC/PWM, RMT and the DHT/1-Wire timing aren't modelled.
  Neither are the BME680 and the SCD30 yet.
- Time runs as fast as QEMU emulates (`-icount` isn't set). Timing tests
  belong on the bench.
- All chips share one eFuse image and so one base MAC address. That is
  fine for one gateway per QEMU user network.
