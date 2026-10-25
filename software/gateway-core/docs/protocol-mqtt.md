# MQTT protocol

The topic scheme and payload formats implemented by `gateway_core::topics`
(and driven by `gateway-hardware::mqtt`/`translate`). Reproduced from the
original C++ `Bridge*.cpp` classes' actual behavior, including quirks
called out explicitly below. See [`protocol-can.md`](../../../docs/protocol-can.md) for
how each of these relates to a CAN message, and
[ADR 0003](../../../docs/adr/0003-mqtt-transport-to-home-automation-hub.md)/
[ADR 0007](adr/0007-mqtt5-client.md) for why MQTT, and which version.

## Connection

The gateway subscribes to its command topics one at a time, waiting for
the broker's SUBACK of each before sending the next SUBSCRIBE (the client
refuses more than four unconfirmed ones).

- Protocol version: **MQTT 5.0** (see [ADR 0007](adr/0007-mqtt5-client.md)
  for why this differs from the original's 3.1.1).
- **Two brokers** can be configured and served at the same time (say,
  Home Assistant's as the primary and a Cerbo's as the secondary), each in
  its own slot with its own connection. Commands are accepted from either
  and handled identically. Everything the gateway publishes (node states,
  sensor readings, its own `available` announcement) goes to the
  **primary** broker only; the secondary broker is receive-only, and
  publishing something to it too is a deliberate exception that has to be
  programmed per message (`mqtt::publish_secondary`; there is none yet).
  Console commands `mqtt-…` configure the primary slot, `mqtt2-…` the
  secondary; a slot that isn't enabled stays idle.
- Broker address: `mqtt://<host>[:<port>]` (port defaults to `1883`,
  unencrypted) or `mqtts://<host>[:<port>]` (port defaults to `8883`,
  [encrypted](#encryption-mqtts)).
- Credentials: optional per-broker username/password, sent as the MQTT
  CONNECT username/password if non-empty.
- Client id: `"gateway-hardware"`, fixed (on each broker).
- `clean_start` is set on every connect — no persisted session across
  reconnects.
- On disconnect, the gateway reconnects with a fixed backoff and
  **resubscribes every topic below from scratch** on each successful
  (re)connect — subscriptions are not assumed to survive a reconnect, so
  this happens unconditionally every time, not just after the first
  connect.
- On every successful (re)connect to the primary broker, the gateway also
  republishes its own
  [`canbus/available/<hex>`](#canbusavailable) self-announcement (see
  below) — this is the *only* case where an `Available` CAN message
  results in an MQTT publish; `Available` frames observed from *other*
  devices are tracked in the node registry but are **not** republished to
  MQTT (a faithfully-reproduced quirk of the original — its equivalent
  code path is unreachable, since it builds the topic string but never
  actually calls publish with it).

## Device addressing in topics

Every state topic below ends in a **device segment**: either the device's
configured custom name, or `0x<hex device key>` (the CAN identifier with
`msg_type` zeroed — see [`protocol-can.md`](../../../docs/protocol-can.md#device-key))
if it has no name. The custom name is preferred whenever the node
registry has one for that device; there's no way to force the numeric
form for a named device.

Command topics take the equivalent identifier as the topic's 3rd
`/`-delimited segment (`canbus/<kind>_command/<target>`) — `<target>` is
resolved the same way, but in reverse: a literal, **lowercase**
`0x`-prefixed hex string parses directly as the device key (`0X` with a
capital X does *not* count — it falls through to a name lookup instead,
which will simply fail to resolve); anything else is looked up by
registered custom name. An unresolvable target is silently dropped — no
error is published anywhere.

## State topics (gateway → broker)

### `canbus/relais_state/<device>`

Body: `<state>` — the raw state byte as a decimal string, e.g. `"3"`.
Published from a `RelaisState` CAN frame.

### `canbus/rollershutter_state/<device>/<bank>/<number>`

Body: `<state>` — the raw state byte as a decimal string. Published from a
`RollershutterState` CAN frame; `<bank>`/`<number>` come from that
frame's payload (see [`protocol-can.md`](../../../docs/protocol-can.md#relayrollershutter-relais--rollershutter--relaisstate--rollershutterstate)).

### `canbus/button/<device>`

Body: `<button_id>/<event>/<count>` — decimal button id, one of
`released`/`hold`/`single`/`double`/`tripple` *(sic)*, decimal count.
Published from a `ButtonEvent` CAN frame; `Pressed` events are decoded
but never published (only settled/terminal states are).

### `canbus/presence/<device>`

Body: `8/<release|hold>/<count>` — the leading `8` is a **literal
constant** (the original's hardcoded "external PIR" id), not the actual
`button_id` field from the frame; every event kind other than `Released`
collapses to `hold`. Published from a `PirSensor` CAN frame.

### `canbus/<sensor>/<device>/0x<sub_id>`

`<sensor>` is one of `temperature`, `pressure`, `humidity`, `co2`, `voc`,
`air_quality` (all fixed-point sensor readings) or `brightness` (ambient
light — this one has **no** `/0x<sub_id>` suffix at all). Body: the
decoded reading as a decimal string (e.g. `"21.5"` for temperature,
divided-by-16 fixed point; a plain integer for brightness, unscaled).

`<sub_id>` for the fixed-point sensors is the CAN payload's 48-bit
accompanying id, but formatted from only its **low 32 bits** — a
faithfully-reproduced quirk of the original's `toHexString(unsigned int)`
silently narrowing a wider value passed to it. A sub-id with any of its
top 16 bits set produces a different-looking topic here than a
"correct" 48-bit-aware formatter would.

### `canbus/available/<hex>`

Body: literal `"0x01"`. Published only as this gateway's own self-
announcement on MQTT (re)connect — see "Connection" above. `<hex>` is
this gateway's own device key, hex, no `0x` prefix and no leading zero
padding.

### `canbus/log/0x<hex>` (planned in the original, not currently wired up)

The original's `Logging` decorator mirrors sent/received CAN frames here
when a runtime "MQTT logging" flag is enabled (toggled via the
`mqtt_logging` `/control.json` verb). Reserved for parity; not yet wired
into `gateway-hardware`'s CAN transport.

### Encryption (`mqtts://`)

`mqtts://` connections use TLS 1.3 (one cipher suite,
`TLS_AES_128_GCM_SHA256`; every current Mosquitto/OpenSSL offers it).
Brokers on a LAN mostly have self-signed certificates and the gateway has
no clock, so instead of validating a certificate chain it *pins the
broker's public key*:

1. `mqtt-trust` (`mqtt2-trust` for the second broker) makes the next
   connection a capture: the certificate the broker presents is stored in
   flash and its SHA-256 fingerprint is logged, to compare with
   `openssl x509 -in <cert> -noout -fingerprint -sha256`. The request is
   not persisted; it is also lost on a restart.
2. From then on the broker must present a certificate with that same
   public key and sign the handshake with the matching private key, or the
   connection is refused. A renewed certificate with the same key is
   accepted; a new key needs `mqtt-trust` again.

`mqtt-untrust` forgets the pinned certificate (a TLS broker without one
isn't connected to), `mqtt-insecure on` accepts any certificate instead:
encrypted, but a man in the middle isn't excluded.

Limits: certificates with ECDSA P-256/P-384 or RSA keys (at most 2048
bytes), and validity dates, the issuer and the host name are not checked.

## Command topics (broker → gateway)

Every command topic below is an MQTT subscription with a `#` wildcard
(e.g. `canbus/relais_command/#`); the device is addressed by the topic's
3rd segment, as described under "Device addressing" above.

### `canbus/relais_command/<device>`, `canbus/rollershutter_command/<device>`

Body: `<num>/<state>/<stop_time_ms>` — three decimal integers, all
required. Encodes to the CAN `Relais`/`Rollershutter` message using the
**MQTT-command wire shape**, distinct from the RPC path — see
[`protocol-can.md`](../../../docs/protocol-can.md#relayrollershutter-relais--rollershutter--relaisstate--rollershutterstate).

### `canbus/heating_target_command/<device>`, `canbus/heating_energy_command/<device>`, `canbus/heating_emergency_command/<device>`

Commands for a `HeatingControl` node (new — the original has nothing like
them). Each becomes the CAN message of the same name, see
[`protocol-can.md`](../../../docs/protocol-can.md#heating-controller-heatingtarget--heatingenergy--heatingemergency).
Bodies are `/`-separated; temperatures are decimal °C (`45`, `45.5`,
`-3.25`, within -50…150), flags are `1`/`0` (or `true`/`false`,
`on`/`off`):

| Topic | Body | Example |
|---|---|---|
| `heating_target_command` | `<flow_target_°C>/<tank_mode>` | `45.5/1` — 45.5 °C flow, supplied from the tank |
| `heating_energy_command` | `<power_W>/<rod_enabled>` | `6200/1` — 6.2 kW surplus, rod released (negative while importing) |
| `heating_emergency_command` | `<stage 0-3>/<tank_target_°C>` | `2/60` — rod stage 2 until the tank is at 60 °C; stage `0` ends it |

The node counts each of these as gone 30 s after it last arrived, and then
switches the heating rod off (or the circuit back to direct operation) —
so the publisher has to repeat them, about every 5 s. The gateway does not
repeat anything itself. **Publish them without the retain flag:** a
retained delivery (what a broker sends after a reconnect) is ignored, since
an old value replayed that way would look fresh to the node. A body that
doesn't parse, or a device that can't be resolved, is dropped silently like
for the other command topics.

While a heating controller is in **manual mode** (set from the web UI or the
node's console, see [`api-rest.md`](api-rest.md)) all three are dropped —
by the gateway, which knows the state from the node's reports, and by the
node itself. The rod is off then, whoever publishes.

### `canbus/lamp_command/<device>`

Body: `<value>/<bitmask>[/<bank>]` — `value` decimal, `bitmask`
**hexadecimal**, `bank` decimal and optional. Encodes to `LampGroup`: the
8-byte form if `bank` was supplied, the 4-byte form otherwise (see
[`protocol-can.md`](../../../docs/protocol-can.md#lamps-lampgroup--nightlight)).

### `canbus/nightlight_command/<device>`

Body: a single decimal integer, the **entire** body (not slash-delimited
like the other command topics). Encodes to the 1-byte `Nightlight`
message.

### `canbus/debug/<target>`

Raw CAN passthrough. `<target>` is a **hex CAN identifier**, not a
device-registry lookup (unlike every other command topic above) — the
full `is_ng|group|device_type|device_id|msg_type` value to send to,
exactly as given. Body: `<count>/<hex-byte-pairs...>` — `<count>` is
parsed as an integer but its value is otherwise unused (a no-op
validation-only field, reproduced as-is); everything after the first `/`
is read as consecutive 2-hex-digit byte pairs, capped at 8 bytes (the CAN
frame limit), and sent verbatim as that CAN message's payload.

### `canbus/gateway/restart`

Exact topic match (no wildcard segment, no device targeting). Any body.
Restarts the gateway itself. A retained message is ignored — it would
restart the gateway again after every reconnect.

## Not covered by this document

The `/control.json` HTTP RPC API (separate from MQTT, and known to need
rework — see
[`technical-debt.md`](technical-debt.md#the-controljson-rpc-api-is-not-good))
and `/state.json`/`/bridge_config.json`/`/update/data` are HTTP, not MQTT,
and are out of scope for this document.
