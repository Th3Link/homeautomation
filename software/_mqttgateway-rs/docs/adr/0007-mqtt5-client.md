# ADR 0007: Use an MQTT 5.0 client (`rust-mqtt`) instead of matching the original's MQTT 3.1.1

**Status:** Accepted

## Context

The original C++ gateway uses ESP-IDF's `esp-mqtt` client, which speaks
MQTT 3.1.1. `gateway-hardware` needs a `no_std`, `embedded-io-async`-based
MQTT client to run under embassy without ESP-IDF underneath it. The one
actively maintained option in that space, `rust-mqtt`, only implements
MQTT 5.0 — its own `v3` Cargo feature exists but is documented as
"Unused" (a placeholder for future work, not a working 3.1.1
implementation). There is no other maintained `no_std`/
`embedded-io-async` MQTT 3.1.1 client to reach for instead.

## Decision

Use `rust-mqtt` and accept speaking MQTT 5.0 to the broker, rather than
matching the original's 3.1.1 wire version.

## Consequences

- Any MQTT broker modern enough to be in real use (Mosquitto, EMQX,
  HiveMQ, a hub's built-in broker, ...) speaks both 3.1.1 and 5.0, so this
  is not expected to be a practical interoperability problem for anyone
  actually deploying this gateway.
- `rust-mqtt`'s API is a lower-level, explicit protocol client (session
  state, QoS, and acknowledgement handling are all caller-visible) rather
  than an opinionated "just works" client — `gateway-hardware`'s
  `mqtt.rs` owns the connect/reconnect loop, resubscription, and
  keep-alive behavior itself; none of that comes for free from the
  library the way it effectively did from `esp-mqtt`.
- If `rust-mqtt` ever adds a real 3.1.1 mode, or a maintained
  alternative appears, revisit — nothing about the topic scheme or
  payload formats in [`../protocol-mqtt.md`](../protocol-mqtt.md) depends
  on which protocol version carries them.
