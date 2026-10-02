# ADR 0001: Split the port into `gateway-core` (logic) and `gateway-hardware` (peripherals)

**Status:** Accepted

## Context

The original C++/ESP-IDF gateway mixes wire-format parsing, MQTT topic
translation, device bookkeeping, and RPC handling directly with ESP-IDF
calls (`nvs_*`, `esp_mqtt_client_*`, `twai_*`, `esp_http_server`, ...)
throughout. There is no host-testable layer at all — every behavior can
only be exercised by flashing real hardware.

The Rust rewrite is a green-field implementation, so this constraint
doesn't have to carry over. `cancomponent-rs` (the sibling CAN-node
firmware project, reference-only in this repository) already established
a working precedent: a `#![no_std]`, hardware-independent crate for wire
format and pure control logic (`cc-core`), separate from the peripheral-
driving firmware crate (`cc-hardware`).

## Decision

Split this port the same way, as two crates in one Cargo workspace:

- **`gateway-core`** — `#![no_std]`, no hardware I/O. CAN wire format,
  MQTT topic construction/parsing, the CAN-node registry, `/control.json`
  RPC-to-CAN-frame encoding, the CAN-bus OTA orchestration state machine,
  and the consolidated configuration schema. Every function is pure:
  callers pass in `now`/state explicitly, nothing reads a clock or touches
  a peripheral. Builds and tests with a plain `cargo test` on any machine.
- **`gateway-hardware`** — the ESP32 firmware binary. Owns every actual
  peripheral (CAN/TWAI, Ethernet/WiFi, MQTT/HTTP sockets, flash-backed
  config storage, OTA, the serial console) and calls into `gateway-core`
  for all translation/encoding decisions.

Unlike `cancomponent-rs` (two independent crates, each with their own
`Cargo.lock`), this workspace uses a single real `[workspace]` root with
one shared lockfile — `gateway-hardware` depends on `gateway-core` as a
normal path dependency.

## Consequences

- The entire CAN↔MQTT translation layer, the RPC-to-CAN encoding, the
  device registry, and the CAN-OTA state machine are unit-tested (114
  tests as of the initial port) without any hardware, emulator, or esp
  toolchain — `cargo test -p gateway-core` runs anywhere.
- `gateway-hardware` still requires the `esp` Rust toolchain channel and
  real (or at minimum a linked, if not flashed) hardware to fully verify,
  but the highest-risk, most detail-sensitive logic (byte-exact wire
  formats, topic quirks, RPC encoding) is proven independently of that.
- Anything that's inherently about *why a byte is the way it is* lives in
  `gateway-core` with a doc comment next to the code; anything about *how
  to actually get that byte onto a wire* lives in `gateway-hardware`. This
  split has to be maintained deliberately — it's easy to accidentally
  leak a peripheral call into what should be pure logic.
