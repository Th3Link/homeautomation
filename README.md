# mqttgateway-rs

ESP32 firmware for a CAN-bus &lt;-&gt; MQTT home-automation gateway — bridging
relay/rollershutter/button/presence/environmental-sensor nodes on a CAN bus
to MQTT, with a local web config UI and both self- and CAN-bus OTA update —
written in Rust (`#![no_std]`, [embassy](https://embassy.dev/) async), plus
the shared, host-testable translation-logic crate it's built on.

This is a from-scratch Rust rewrite of an existing C++/ESP-IDF gateway
(kept elsewhere as local reference, not part of this repository), aiming
for the same runtime behavior — including its exact MQTT topic scheme and
CAN wire format — so it's a drop-in replacement for nodes and MQTT clients
already talking to the original.

## Repository layout

- [`gateway-core/`](gateway-core) — `#![no_std]` CAN wire format (message
  layouts, ID bit-math), MQTT topic translation, the CAN-node registry,
  `/control.json` RPC encoding, CAN-bus OTA orchestration, and the
  consolidated configuration schema. No hardware dependencies, so it
  builds and tests on any machine with a normal stable Rust toolchain:
  ```bash
  cargo test -p gateway-core
  ```
- [`gateway-hardware/`](gateway-hardware) — the actual ESP32 firmware
  binary. Requires the `esp` Rust toolchain channel (see below).

## What the gateway does

- Bridges CAN traffic to MQTT and back: relay/rollershutter state and
  commands, lamp/nightlight commands, button and presence events,
  environmental sensor readings, and a raw debug passthrough — under the
  `canbus/...` topic tree.
- Serves a local web UI (config + live status + firmware upload) with a
  JSON RPC API (`/control.json`) for the same operations plus device
  provisioning (assigning a node its id/type/custom name).
- Updates firmware two ways: itself, over HTTP; or a CAN node, by pushing
  the image over the bus via the `FlashStart`/`FlashWrite`/.../
  `FlashComplete` protocol.

Unlike the original, every web endpoint requires authentication
unconditionally — the original left `/state.json` and the firmware-upload
endpoint completely open, and only enforced Basic Auth on the rest when a
username happened to be configured. That's the one deliberate behavioral
difference from the original; everything else (topic names, wire formats,
defaults, the assorted quirks pinned by this crate's regression tests) is
reproduced as-is.

## Hardware

The gateway sits on a CAN (TWAI) bus shared with relay/rollershutter/
button nodes (see [`cancomponent-rs`](https://github.com/mier88/cancomponent-rs)
for one such node firmware, speaking the same CAN wire format), and reaches
MQTT/the web UI over the network — normally wired Ethernet (the ESP32's
built-in EMAC, over RMII to an external PHY), since these gateways are
typically installed hardwired next to the CAN bus wiring. WiFi is a
fallback for installs without a wired drop, automatically used only when
no Ethernet link comes up — the two can't safely run at once on this board
(its RMII reference clock is APLL-derived, which esp-hal's own docs warn
is unstable while WiFi is active), so the gateway never brings both up
together.

## Building & flashing

`gateway-hardware` targets Xtensa and needs Espressif's Rust toolchain
fork, installed via [`espup`](https://github.com/esp-rs/espup):

```bash
cargo install espup
espup install
source "$HOME/export-esp.sh"   # needed once per new shell
```

Then, from `gateway-hardware/`:

```bash
cargo run --release
```

`cargo run` flashes over USB serial and opens a monitor, via `espflash`
(configured as the custom runner in `gateway-hardware/.cargo/config.toml`).

## Commissioning console

Connect a serial terminal to UART0 at 115200 baud and press Enter to open
the console. `help` lists all commands, including device identity
(`device-id`, `device-type`, `custom-string`, `hwrev`) and NVS
inspection/config commands.

## Firmware updates

The gateway itself can be updated the same USB/`espflash` route used for
initial flashing, over HTTP via the web UI, or (as the tool doing the
pushing) drive a CAN node through the same `FlashStart`/`FlashWrite`/.../
`FlashComplete` protocol `cancomponent-rs` implements on the receiving
end.

## CI

- **CI** — `cargo fmt` / `cargo clippy` for both crates, `cargo nextest`
  with a coverage report and an MSRV check for `gateway-core`.
- **Security audit** — [`cargo-audit`](https://github.com/rustsec/rustsec)
  against the RustSec advisory database, on every dependency change and
  once daily.
- **Renovate** — automated dependency-update PRs, once daily.

## License

GNU General Public License v3.0 (or later) — see [LICENSE.md](LICENSE.md).

This is meant to be freely buildable, flashable, and modifiable hardware
and firmware. GPLv3 was chosen deliberately: its copyleft means anything
built on this stays open, and its Section 6 "Installation Information"
requirement means anyone who conveys a modified version as part of a
physical product also has to provide what's needed to install your own
modified firmware on it — nobody gets to lock you out of hardware you own.

```
mqttgateway-rs — CAN-bus/MQTT home-automation gateway firmware
Copyright (C) 2026  Marc-Hendric Luehr

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
GNU General Public License for more details.
```
