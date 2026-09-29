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

## Message types

`ICAN::MSG_ID_t` / `gateway_core::can_message_type::CanMessageType`. Fixed
discriminants — gaps (23–29, 43–89, 91–127, 142–149, 159–254) are
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
| 19 | `FlashWrite` | 255 | `InvalidMessage` (decode fallback only, never sent) |
| 20 | `FlashVerify` | | |
| 21 | `FlashProgress` | | |
| 22 | `FlashComplete` | | |
| 30 | `ButtonEvent` | | |
| 31 | `TemperatureSensor` | | |
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

Ambient light (`AmbientLightSensor`, and nominally
`AmbientLightSensorWhite` though nothing in the live gateway actually
dispatches that message type) — 4 bytes, raw and **unscaled**:

```text
byte:  0..3
      ┌──────────┐
      │ value    │
      │ u32, LE  │
      └──────────┘
```

## OTA (`FlashStart`/`FlashWrite`/.../`FlashComplete`)

The sequence a bus master (this gateway, as `CANUpdate`) drives to push
firmware to a target node or type-broadcast. **Known to be unreliable —
see [`technical-debt.md`](technical-debt.md#can-bus-ota-is-unreliable) and
[ADR 0010](adr/0010-can-ota-rework-planned.md).** Documented here exactly
as currently implemented, as the baseline for that rework.

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
