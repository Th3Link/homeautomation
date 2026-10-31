# CAN protocol

The wire format implemented by `gateway_core::can_id`, `can_message_type`,
`device_type`, `relais`, `lamp`, `button`, `sensor`, `error`, `device_message`,
and `can_ota`. Reproduced bit-for-bit from the original C++ gateway
(`library/esp32-ha-lib/ICAN.hpp` and the `Bridge*.cpp`/`CANUpdate.cpp`
classes that build/parse these payloads) — see
[ADR 0005](adr/0005-reimplement-wire-protocol-independently.md) for why
this is an independent reimplementation rather than a shared dependency.

## Bus parameters

Standard CAN 2.0B, **29-bit extended identifiers**, not standard (11-bit)
frames — a received standard-ID frame is logged and ignored. Bitrate is
one of four fixed options (`ICAN::BITRATE_t`), persisted but not (yet)
dynamically applied to the peripheral in this port:

| Value | Bitrate |
|---|---|
| `B22_222` (1) | 22.222 kbit/s |
| `B25` (2) | 25 kbit/s |
| `B50` (3) | 50 kbit/s (default) |
| `B100` (4) | 100 kbit/s |

The gateway runs its transceiver **promiscuously** (no hardware
acceptance-filter narrowing) — it needs to see every device's traffic to
bridge it, unlike a single-purpose node that only cares about frames
addressed to itself.

## 29-bit identifier layout

MSB first, 29 bits total:

```text
bit:   28    27..22    21..16        15..8        7..0
      ┌────┬─────────┬────────────┬────────────┬──────────┐
      │is_ng│  group  │device_type │ device_id  │ msg_type │
      │ 1b  │   6b    │    6b      │    8b      │   8b     │
      └────┴─────────┴────────────┴────────────┴──────────┘
```

- **`is_ng`** (1 bit) — "next-gen" protocol marker. Always `1` for
  anything this gateway sends.
- **`group`** (6 bits) — reserved for grouping/broadcast domains; always
  `0` in practice.
- **`device_type`** (6 bits) — see [Device types](#device-types) below.
- **`device_id`** (8 bits) — target/source device id; `0` is the
  broadcast address (never a real device's own id).
- **`msg_type`** (8 bits) — see [Message types](#message-types) below.

As a `u32`: `is_ng<<28 | group<<22 | device_type<<16 | device_id<<8 | msg_type`.

A frame's **device key** is this identifier with `msg_type` zeroed
(`id & 0xFFFFFF00`) — the CAN node registry and MQTT topic builders key
entries by this, since it identifies "which device", independent of
"which message from it".

## Device types

`ICAN::DEVICE_t` / `gateway_core::device_type::DeviceType`. Fixed
discriminants, gaps included:

| Value | Name |
|---|---|
| 0 | `Unknown` (also the fallback for any unrecognized byte) |
| 2 | `LegacyRelais` |
| 3 | `LegacyLamps` |
| 4 | `Button` |
| 5 | `Relais` |
| 6 | `Gateway` (this device's own type, when it addresses itself) |
| 7 | `Rollershutter` |
| 8 | `SSR` |
| 9 | `HeatingControl` |

## Message types

`ICAN::MSG_ID_t` / `gateway_core::can_message_type::CanMessageType`. Fixed
discriminants — gaps (23–29, 43–89, 91–127, 142–149, 159, 163–254) are
intentional, not omissions:

| Value | Name | Value | Name |
|---|---|---|---|
| 0 | `Available` | 90 | `LampGroup` |
| 1 | `DeviceError` | 128 | `PirSensor` |
| 2 | `Restart` | 129 | `HumiditySensor` |
| 3 | `DeviceUid0` | 130 | `Relais` |
| 4 | `DeviceUid1` | 131 | `RelaisState` |
| 5 | `DeviceIdType` | 132 | `Rollershutter` |
| 6 | `DeviceGroup` | 133 | `RollershutterState` |
| 7 | `ApplicationVersion` | 134 | `RelaisMode` |
| 8 | `Baudrate` | 140 | `AmbientLightSensor` |
| 9 | `Uptime` | 141 | `AmbientLightSensorWhite` |
| 10 | `CustomString` | 150 | `Nightlight` |
| 11 | `PwmFrequency` | 151 | `PressureSensor` |
| 12 | `RequestParameter` | 152 | `Co2Equivalent` |
| 13 | `ApplicationVersionString` | 153 | `VocBreath` |
| 14 | `UpdateSilence` | 154 | `AirQuality` |
| 15 | `FlashStart` | 155 | `LogDownload` |
| 16 | `FlashSelect` | 156 | `Ping` |
| 17 | `FlashErase` | 157 | `PingDisable` |
| 18 | `FlashRead` | 158 | `Echo` |
| 19 | `FlashWrite` | 165 | `HeatingCommand` |
| 20 | `FlashVerify` | 166 | `HeatingState` |
| 21 | `FlashProgress` | 167 | `SensorConfig` |
| 22 | `FlashComplete` | 168 | `OtaControl` |
| 30 | `ButtonEvent` | 169 | `OtaData` |
| 31 | `TemperatureSensor` | 255 | `InvalidMessage` (decode fallback only, never sent) |
| 41 | `HwRev` | | |
| 42 | `ExtensionMode` | | |

An unrecognized `msg_type` byte decodes to `InvalidMessage` rather than
failing to decode at all — this makes "ignore an unknown message" an
explicit, matchable case (the original's C++ `switch` just silently falls
through with no default case).

## `UpdateSilence` — bus quieting during OTA

Payload: 1 byte, non-zero = silence on, `0` = off. Received on **any**
device (this gateway included): while silenced, sending any message with
`msg_type > FlashVerify` (i.e. everything except `Available` through
`FlashVerify`, ordinals 0–20) is suppressed — flash/update traffic still
gets through, everything else (sensor readings, button events, `Echo`,
...) is dropped at send time. This lets a bus master push firmware
without other traffic competing for bandwidth.

## `Echo` — loopback probe

Payload: whatever was sent, 0–8 bytes, any content. On receipt, the exact
same CAN id and payload is immediately resent onto the bus, unconditionally
(not gated by `UpdateSilence`) — a round-trip latency/connectivity probe.

A `cancomponents` node also sends an `Echo` request every 30 s and restarts
itself if none comes back for several intervals (the bus is broken or the
gateway is gone). The console's `echo-restart off` switches the restart off
(persisted); the node keeps probing but no longer resets.

## `DeviceIdType` — device provisioning

Payload (2 bytes): `[device_id, device_type]`. Assigns a node's identity.

## `DeviceError` — self-reported errors

Payload (8 bytes): `[component, reserved, reserved, reserved, detail(LE u32)]`.

`component` (`ICAN::ERROR_t`):

| Value | Name |
|---|---|
| 0x01 | `FlashOverrun` |
| 0x02 | `NoConfig` |
| 0x03 | `DeviceIdTypeError` |
| 0x04 | `FirmwareCorrupt` |
| 0x05 | `Can` |
| 0x06 | `Light` |
| 0x07 | `Relais` |
| 0x08 | `Main` |
| 0x09 | `Update` |
| 0x0A | `Nightlight` |
| 0x0B | `Ambient` |

The only live producer is a CAN driver's own bus-alert reporting
(`component = Can`, `detail` = the TWAI alert bitflags that triggered it).
Consumers (the node registry) only ever read byte 0 for display — they
don't decode the rest.

### Structured reports from `cancomponents` nodes

The `cancomponents` firmware doesn't use the layout above; its reports are
`[component, code, severity, local_code, details × 4]`. The gateway only
looks at byte 0 either way, so its values are *not* the legacy
`ICAN::ERROR_t` ones (e.g. `Relais` is 6 here, `Light` in the table above).

| `component` | Name | | `code` | Name |
|---|---|---|---|---|
| 1 | `Can` | | 1 | `InvalidData` |
| 2 | `Device` | | 2 | `SensorLost` |
| 3 | `Update` | | 3 | `SensorRecovered` |
| 4 | `Storage` | | | |
| 5 | `Ota` | | 4 | `OverTemperature` |
| 6 | `Relais` | | 5 | `Timeout` |
| 7 | `Sensor` | | | |
| 8 | `Heating` | | | |

`severity`: 1 `Warning`, 2 `RecoverableError`, 3 `RepeatingError`,
4 `Error`, 5 `CriticalError`.

**`Sensor` reports** concern DS18B20 temperature sensors. `local_code` is
the sensor's slot (order of discovery since boot) and `details` the first
four serial bytes, i.e. the `sub_id` low 32 bits little-endian — the same
id the gateway puts in the sensor's MQTT topic.

- `SensorLost` / `Error`: the sensor failed three measurement cycles
  (30 s) in a row. Repeated as `SensorLost` / `RepeatingError` every minute
  while it stays lost.
- `SensorRecovered` / `Warning`: a lost sensor answers again.

A lost sensor's `TemperatureSensor` frames stop (no stale value is
resent). A sensor that was just found, or that comes back after being lost,
answers with its power-on value of 85 °C until its first real conversion,
so it is sampled every 3 s until three readings in a row agree (within
0.5 K) before its values are sent or reported as recovered. The list of
sensors is only kept in RAM: a sensor that is dead at boot is never seen.

## Heating controller (`HeatingCommand` / `HeatingState`)

A `HeatingControl` node (device type 9) has the relay hardware of a
`Relais` node and drives it itself — a tank with a three-stage heating
rod, two 3-way valves, a mixer and a pump. CAN `Relais` commands for its
relays 0-10 are ignored (relay 11 is not part of the controller). It is
controlled by **one message type**, `HeatingCommand` (165), sent to the
node's device id: the first byte holds an operation code (bits 0-2) and, in
the bits above it, that operation's flags; the other bytes the values.
Temperatures are signed 1/16 °C values like the sensor messages; multi-byte
values are little-endian; extra bytes are ignored.

| Op | Command | Byte 0, bits 3-7 | Bytes 1… |
|---|---|---|---|
| 0 | Target | bit 3: supply the circuit from the tank instead of directly from the heat pump | flow target `i16` |
| 1 | Energy | bit 3: the heating rod is released | available (feed-in) power `i32` W; negative while importing |
| 2 | Emergency | bits 3-4: rod stage 0-3 (0 = no emergency) | tank target `i16`: run the rod at that stage regardless of the available power, until the tank reaches it |
| 3 | Manual | bit 3: manual mode on; bit 4: a relay is given; bits 5-7: relay − 6 | bit 0: the relay is on |

An unknown op, a short frame, or a manual relay outside 6-10 is dropped and
reported (`InvalidData`).

**Manual mode** is for commissioning (venting, valve checks): it takes the
relays 6-10 (valves, mixer, pump) out of the controller's hands. With bit 3
the relays all go off when it starts and are then switched one at a time
(never both mixer directions); without it the controller has them back, the
mixer finding its position anew. The rod (relays 0-5) can't be reached. There
is no time limit — it stays on until it is ended (a restart of the node ends
it too). While it is on, the other commands are dropped, whoever sent them, and
the ones received before are forgotten, so the rod switches off at once.

**State.** `HeatingState` (166) is *read*, never sent by the node by itself: an
R frame on it is answered with a data frame, `[flags][relays]`. Byte 0: bit 0
manual mode on, bits 1-3 the target/energy/emergency input is alive, bit 4
over-temperature latch, bits 5-6 rod stage. Byte 1: bit 0 = relay 6 … bit 4 =
relay 10 for the relays that are on in manual mode.

The retired numbers 160-164 (the five messages this replaced) are never to be
used again: a node that was not updated yet would read a frame of the new
kind as the old one. A mismatch of versions is safe: a node that gets
nothing it understands counts the inputs as gone.

The inputs (target, energy, emergency) must be repeated at least every 30 s
(the intended rate is 5 s); **30 s without one counts as the input being
gone** — without energy/emergency all rod stages switch off, without target
the valves and the pump return to direct operation.

The gateway sends them on behalf of MQTT topics
`canbus/heating_{target,energy,emergency}_command/<device>`; see
[`protocol-mqtt.md`](protocol-mqtt.md).

## Sensor management (`SensorConfig`)

`SensorConfig` (167) manages the temperature sensors of a node: the list, their
names and their roles in the heating controller. A sensor is called by its
**slot** (0-31), the number in the order the node found its sensors since it
started. A slot is local to the node and temporary — after a restart, or when
sensors come and go, it may mean another sensor — so a command that matters
carries a *guard*, the first four serial bytes the sender thinks the slot
has, and the node refuses it if that is not so. Names stay with the sensor
(the node keeps them by serial), slots don't.

Byte 0 of every frame: bits 0-4 slot, bits 5-7 operation.

| Op | Frame | Bytes 1-7 |
|---|---|---|
| 0 name | set (to the node) / reported (from it) | the name: 1-7 characters out of `A-Z a-z 0-9 - _`, NUL padded; unique on a node |
| 1 info | request for a slot (1 byte long) / the answer | serial (6 bytes, as in `TemperatureSensor`), flags: bit 0 lost, bit 1 settling, bits 4-7 role |
| 2 role | give the sensor a role | role (1 byte), guard (4 bytes) |
| 3 forget | forget the sensors that are lost | – |

Roles: 0 none, 1-7 tank position 0-6 (top to bottom), 8 flow. Giving a role
takes it from the sensor that had it, and a sensor has one role. An **R frame**
on `SensorConfig` asks for the whole list: the node answers with an info frame
and, if the sensor has a name, a name frame for each of its sensors, a few
milliseconds apart. A refused request is reported as `InvalidData`
(`Component::Sensor`) with the slot and a reason number.

**Rod stages.** Each stage adds 3 kW: stage 1 = relays 0+1, stage 2 =
0-3, stage 3 = 0-5 (always in pairs). Normally the stage follows the
available power (stage *n* starts at *n* × 3000 W + 300 W and keeps running down to *n* × 3000 W),
as long as the rod is released and the tank's reference sensor is below
the tank target. Discretionary stage changes happen at most every 120 s;
anything unsafe or pointless switches the rod off at once: a tank sensor
at 80 °C (re-armed at 75 °C), no usable tank sensor, missing input
messages, a withdrawn release, a full tank. This is in addition to, not
instead of, the independent hardware protection (contactor at 85 °C, STB
at 90 °C).

**Circuit.** In tank operation both 3-way valves (relays 6 and 7) switch
together; the circuit pump (relay 10) follows 2 minutes later and stops
together with the valves. The mixer (relays 8 = hot, 9 = cold; 90° in
120 s) regulates the flow temperature to the target with a stepwise
controller (a pulse, then 60 s for the sensor to follow) while the tank is
hot enough; if it isn't, the mixer goes to the hot end (full flow). In
direct operation, and while no `HeatingTarget` arrives, it goes all the way
to return (cold end), the safe position. After a restart it first drives
to the cold end to find its position.

Relay 11 (hot-water circuit pump) is not touched by the controller; it
stays available to ordinary `Relais` commands over CAN and MQTT.

**Faults** are reported as `DeviceError` with `Component::Heating`:
`OverTemperature` (details: hottest tank temperature), `SensorLost` (no
usable tank sensor while heat is wanted), `Timeout` (the power messages
stopped), `InvalidData` (a malformed message; `local_code` is the
message type). Persisting faults are repeated every minute.

The sensors can be named (`sensor-name`) and given roles from the web UI or
over CAN as well (`SensorConfig`); the console's `sensors` shows slot, name
and role. The console of a `HeatingControl` node has `tank-sensor`, `flow-sensor`,
`tank-target` (default 75 °C), `tank-ref` and `heating` to assign the
sensors (by the id `sensors` prints), set the charge target and the tank
position it is compared with (default: the bottom one, position 6), and to
see what the controller is doing and why. Until a reference sensor is
assigned, the rod stays off.

For commissioning, `heating-manual on` hands the relays 6-10 (valves, mixer,
pump) over to the console: they all go off and `relais <6-10> on|off`
switches them one at a time (the two mixer directions never both on). The
heating rod, relays 0-5, always stays with the controller and its limits.
`heating-manual off` (or a restart) switches the relays 6-10 off again and
the controller takes over, with the mixer finding its position anew; there
is no time limit. Relay 11 can be switched with `relais 11` at any
time.

## Relay/rollershutter (`Relais` / `Rollershutter` / `RelaisState` / `RollershutterState`)

**Two different wire shapes exist for a *command*, depending on which
gateway entry point built it** — reproduced as-is (not unified), since
real nodes only ever see whatever the deployed gateway actually sends:

**MQTT-originated command** (`canbus/relais_command/#` /
`canbus/rollershutter_command/#`) — 8 bytes, no bank field:

```text
byte:  0     1      2..5              6  7
      ┌────┬──────┬─────────────────┬──┬──┐
      │num │state │ stop_time_ms    │00│00│
      │ u8 │ u8   │ u32, LE         │  │  │
      └────┴──────┴─────────────────┴──┴──┘
```

**`/control.json` RPC command**, and every **state report**
(`RelaisState`/`RollershutterState`) — 8 bytes, matching
`ICAN::RELAIS_MSG_t`'s packed-bitfield layout:

```text
byte:  0       1      2..4               5      6  7
      ┌───────┬──────┬──────────────────┬──────┬──┬──┐
      │number │state │ time_ms          │ bank │00│00│
      │ u8    │ u8   │ u24, LE          │ u8   │  │  │
      └───────┴──────┴──────────────────┴──────┴──┴──┘
```

`time_ms` truncates to 24 bits (~4.6h max) — a hardware/protocol limit,
not a software choice. A 6-byte frame (just `number..bank`, no reserved
bytes) decodes the same way.

`state`/`num`/`number` are raw bytes — there's no enum for relay state on
the wire; callers interpret `0`/`1`/`2`/`3` as
off/up/down/on by convention only.

**`RelaisState` reports are simpler still**: only byte 0 (the state) is
read; nothing else in the frame is interpreted, even though a sender
might put more there.

## Lamps (`LampGroup` / `Nightlight`)

`LampGroup`, matching `ICAN::LAMP_MSG_t` — **two lengths**, both valid:

Full (8 bytes, sent when a `bank` was specified):

```text
byte:  0      1..3               4      5  6  7
      ┌──────┬──────────────────┬──────┬──┬──┬──┐
      │value │ bitmask          │ bank │00│00│00│
      │ u8   │ u24, LE          │ u8   │  │  │  │
      └──────┴──────────────────┴──────┴──┴──┴──┘
```

Short (4 bytes, sent when no `bank`; `bank` decodes as `0`):

```text
byte:  0      1..3
      ┌──────┬──────────────────┐
      │value │ bitmask          │
      │ u8   │ u24, LE          │
      └──────┴──────────────────┘
```

The `/control.json` RPC path **always** sends the 4-byte short form, even
though its request body accepts a `bank` field — that field is parsed but
never actually placed on the wire from that entry point. The MQTT command
path (`canbus/lamp_command/#`) sends whichever form matches whether its
body supplied a bank.

`Nightlight` — 1 raw byte, no framing.

## Buttons and presence (`ButtonEvent` / `PirSensor`)

Both message types share one 4-byte payload:

```text
byte:  0          1      2..3
      ┌──────────┬──────┬──────────┐
      │button_id │event │ count    │
      │ u8       │ u8   │ u16, LE  │
      └──────────┴──────┴──────────┘
```

`event` (`ICAN::BUTTON_EVENT_t`):

| Value | Name |
|---|---|
| 0 | `Released` (also the fallback for any unrecognized byte) |
| 1 | `Pressed` |
| 2 | `Hold` |
| 3 | `Single` |
| 4 | `Double` |
| 5 | `Tripple` *(spelling matches the original wire-protocol discriminant name — it's a wire contract, not prose)* |

`ButtonEvent` publishes a settled/terminal state only — `Pressed` is
decoded but never republished to MQTT. `PirSensor`'s rendering hardcodes
button id `8` in its MQTT body regardless of the frame's actual
`button_id` field (a faithfully-reproduced quirk — see
[`protocol-mqtt.md`](protocol-mqtt.md#canbuspresence)), and collapses
every event kind other than `Released` to a single "hold" state.

How the node sends them (the original firmware's behaviour, kept by the
Rust node firmware, `cancomponents-core/src/button_fsm.rs` and
`presence.rs`):

- **Buttons** 0-3 on the board, 4-7 on the extension (`ExtensionMode`
  button), polled every 10 ms. A press sends `Pressed` (count 0); 50 ms
  after it the level is ignored. Releasing opens a 250 ms window: `Single`
  or `Double` when it closes, `Tripple` right at the third release (count
  0); further presses within a click sequence aren't sent. Held, the
  button sends `Hold` from 2 s on every 500 ms with count 1, 2, …, and
  `Released` with the last count when let go.
- **Presence** (`PirSensor`, `button_id` 8): once a second while there is
  motion, `Pressed` with count 1, then `Hold` with count 2, 3, …; the
  first second without motion sends `Released` with count 0.

## Environmental sensors

Fixed-point readings (`TemperatureSensor`, `PressureSensor`,
`HumiditySensor`, `Co2Equivalent`, `VocBreath`, `AirQuality`) — 8 bytes:

```text
byte:  0..5               6..7
      ┌──────────────────┬──────────┐
      │ sub_id            │ raw_value│
      │ u48, LE           │ u16, LE  │
      └──────────────────┴──────────┘
```

Decoded value = `raw_value / 16.0` (fixed-point, ÷16 scale).

### `TemperatureSensor` from DS18B20 sensors

Relais devices with DS18B20 sensors on their 1-Wire header send one
`TemperatureSensor` frame per sensor. `sub_id` is the sensor's 48-bit
serial (ROM bytes 1..=6, without the family code and CRC byte) and
`raw_value` is the sensor's native 1/16 °C reading. Every sensor is
measured every 10 s; the frames go out every 30 s by default (CLI:
`sensor-interval`). The layout is the one the original C++ firmware used.

`raw_value` is a signed two's-complement value for `TemperatureSensor`
(below-zero temperatures), which the gateway decodes accordingly
(`FixedPointReading::signed_value`). The other fixed-point sensors are
unsigned.

### Temperature, humidity and pressure from a BME680

The light switch's sensor board (`ExtensionMode` sensors) sends its
BME680's temperature (°C, signed), humidity (%) and pressure (hPa) as
`TemperatureSensor`, `HumiditySensor` and `PressureSensor` once per send
interval (`sensor-interval`, default 30 s). `sub_id` is `0x6800_00xx`, the
low byte the chip's I2C address (0x76 or 0x77): topic id `0x68000077`.

On the gateway the sensor shows up as `canbus/temperature/<device>/0x<id>`,
where `<id>` is the low 32 bits of `sub_id` in lowercase hex — for a sensor
with ROM `28 22 B0 C3 32 20 01 36` that is `0x32c3b022`. The node's
`sensors` console command prints this id next to each sensor's ROM and
current temperature, to map physical sensors to topics.

Ambient light (`AmbientLightSensor`, and nominally
`AmbientLightSensorWhite` though nothing in the live gateway actually
dispatches that message type) — 4 bytes. The Rust node firmware sends lux
from the sensor board's VEML7700, measured every 2 s in one of two ranges
(up to ~550 lx finely, up to ~35 klx coarsely), on a change of more than
10 % (at least 2 lx) and otherwise once per send interval
(`cancomponents-core/src/ambient_light.rs`). The gateway passes the value
on unscaled:

```text
byte:  0..3
      ┌──────────┐
      │ value    │
      │ u32, LE  │
      └──────────┘
```

## Firmware update in blocks (`OtaControl` / `OtaData`)

The way the `cancomponents` nodes are updated; the nodes know no other. Design
and reasons: [ADR 0017](../software/gateway-core/docs/adr/0017-block-ota.md).
The image goes over in **blocks** of 1 KiB (`BLOCK_SIZE`, a constant: a multiple of
256 that divides the 4 KiB flash sector). A block is announced by a header
frame and followed by its bytes in `OtaData` (169) frames, 8 bytes each (the last
one zero-padded), sent back to back with no pause. All control frames are
`OtaControl` (168), the operation in the low 4 bits of byte 0:

| Op | Frame | Direction | Layout |
|---|---|---|---|
| 0 | Begin | to the node | byte 0 high nibble: block size / 256 − 1; bytes 1-3 image size (u24); bytes 4-7 image CRC-32 |
| 1 | Block | to the node | bytes 1-2 index (u16); bytes 3-4 length (u16, ≤ block size, only the last block is shorter); bytes 5-6 CRC-16/CCITT-FALSE of the block |
| 2 | End | to the node | – (verify the whole image and switch to it) |
| 3 | Abort | to the node | – |
| 4 | Ack | from the node | bytes 1-2: the block expected next (cumulative); `Ack 0` answers `Begin` |
| 5 | Nak | from the node | bytes 1-2 block index; byte 3 reason: 1 frames missing, 2 bad CRC, 3 flash failed, 4 bad header, 5 out of order (the index is the one expected) |
| 6 | Done | from the node | – (the image verified; the node restarts about a second later) |
| 7 | Error | from the node | byte 1: 1 bad `Begin`, 2 image too large, 3 not started, 4 verify failed, 5 flash failed, 6 `End` before the last block |

The node acknowledges a block when its length and CRC-16 are right and it is
safely in its sector buffer; the block that completes a 4 KiB sector is
acknowledged after the sector was written. A block that does not come out
right is answered with a `Nak`; a block that stops (300 ms without a frame) is
asked for again by a `Nak`; the sender repeats a refused or unanswered block
(1 s) up to 8 times, then gives the update up with `Abort`. A repeated block
whose ack was lost is acknowledged again and not taken twice. `Begin` is
answered within 10 s, `End` within 15 s. The node silences itself for the
update (the update messages are exempt from `UpdateSilence`), and a new image is
only kept once the node has been heard on the bus (its first `Echo` answered).

## Legacy OTA (`FlashStart`/`FlashWrite`/.../`FlashComplete`)

The original streaming protocol, kept for the old STM32 nodes and as the
gateway's fallback and for updates by type; the `cancomponents` nodes no longer
implement it. **Known to be unreliable**, see
[`technical-debt.md`](technical-debt.md#can-bus-ota-legacy-streaming-protocol).
Documented here as implemented in the gateway (`gateway_core::can_ota`): the
sequence a bus master (this gateway, as `CANUpdate`) drives to push firmware to
a target node or type-broadcast.

Start sequence (`filesize`, `crc` known up front):

| Step | Wait before | Message | Payload | RTR |
|---|---|---|---|---|
| 1 | — | `Restart` | `[2]` (`AVAILABLE_t::UPDATE_MODE`) | no |
| 2 | 2000ms | `FlashSelect` | `[0,0,0,0, filesize(BE u32)]` | no |
| 3 | — | `FlashStart` | `[crc(BE u32), filesize(BE u32)]` | no |
| 4 | 500ms | `FlashErase` | *(empty)* | no |
| 5 | 5000ms | `FlashVerify` | *(empty)* | **yes** |
| — | 500ms | *(no send — trailing delay before streaming data)* | | |

Data streaming, repeated until `filesize` bytes are sent: wait
`update_delay_ms` (configurable, default 10ms) before each `FlashWrite`
frame carrying up to 8 bytes; every 4096 cumulative bytes sent, an extra
1000ms pause follows that frame.

Completion sequence:

| Step | Wait before | Message | Payload | RTR |
|---|---|---|---|---|
| 1 | 1000ms | `FlashComplete` | *(empty)* | no |
| 2 | — | `FlashVerify` | *(empty)* | **yes** |
| 3 | 2000ms | `Restart` | `[1]` (`AVAILABLE_t::APPLICATION`) | no |

"Abort" is not a distinct sequence — it runs the exact same completion
sequence as a successful finish.

Every step in both sequences is a **raw-addressed** send (the full target
CAN id, `base_key + msg_type`, computed by the sender) — never gated by
`UpdateSilence` (see that section above; the silence gate only applies to
a device's *own*-identity-composed sends, not these).

Targeting: `by_type_start` addresses `0x10000000 | (device_type << 16)`
(broadcast to every device of that type, `device_id = 0`); `by_uid_start`
addresses a single already-resolved device key directly.
