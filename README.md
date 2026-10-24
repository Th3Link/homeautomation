# homeautomation

Components for a CAN-bus home-automation system: relay/rollershutter/
light/button/sensor nodes on a shared CAN bus, bridged to MQTT (and from
there to openHAB or Home Assistant) by a gateway. This repository holds
the hardware (KiCad), mechanics (3D-printable enclosures), and firmware
for the whole system.

**Start here:** [`docs/index.md`](docs/index.md) — the full documentation
index (requirements, architecture decisions, protocol specs), also built
into every crate's `cargo doc` output and published to GitHub Pages.

## Software

The firmware is Rust (`#![no_std]`, [embassy](https://embassy.dev/) async,
[esp-hal](https://github.com/esp-rs/esp-hal)), organized as one Cargo
workspace under `software/`:

- [`common-core`](software/common-core) / [`common-hardware`](software/common-hardware)
  — CAN wire-format types and ESP32 hardware-support helpers shared by
  both firmware lines.
- [`gateway-core`](software/gateway-core) / [`gateway-hardware`](software/gateway-hardware)
  — the CAN↔MQTT bridge: web config UI, `/control.json` RPC, self- and
  CAN-bus OTA update, over wired Ethernet (DHCPv4 + SLAAC, no WiFi).
- [`cancomponents-core`](software/cancomponents-core) / [`cancomponents-hardware`](software/cancomponents-hardware)
  — the node firmware living on the bus: relays, rollershutters, buttons,
  lights.
- [`tools/mqtt-log-reader`](software/tools/mqtt-log-reader) — a small
  Python tool for reading back the gateway's MQTT log/debug topics.
- [`legacy/`](software/legacy) — the original C++/ESP-IDF implementation,
  kept for reference. No longer built by CI.

### Building

The `*-core` crates are hardware-independent and build/test on any
machine with stable Rust:

```bash
cargo test -p gateway-core -p cancomponents-core -p common-core
```

The `*-hardware` crates target Xtensa (ESP32) and need Espressif's Rust
toolchain fork, installed via [`espup`](https://github.com/esp-rs/espup):

```bash
cargo install espup
espup install
source "$HOME/export-esp.sh"   # needed once per new shell
```

Then, from a `*-hardware` crate directory (e.g. `software/gateway-hardware/`):

```bash
cargo run --release
```

`cargo run` flashes over USB serial and opens a monitor, via `espflash`
(configured as each crate's custom runner in its own `.cargo/config.toml`).

## Hardware

### Relais

Disclaimer: if you put mains voltage on a relais board, make sure you
know what you are doing. These components are experimental and you are in
charge of your own actions. Be safe — if you're unsure, get an expert or
leave it.

Three PCBs, one shared firmware, to switch power on and off:

- **Solid-state relais (SSR)** — mains voltage only (~230V @ 50-60Hz), up
  to 1A per relay, six relais sharing one 1A fuse. Be careful with
  capacitive/inductive loads; handles small rollershutters, heating
  actuators, and electro-magnetic contactors fine.
- **Regular relais** — up to 16A each, no on-board fuse (protect your own
  circuits). No arc-quenching chamber, so an electro-magnetic contactor is
  a better choice for heavy inductive loads (motors, frequency drives).
- **Rollershutter** — two cascaded relais per channel (power, then
  up/down), with software interlocking so up and down are never active
  at once.

All three use the same firmware and the same ESP32 pinning, and each has
one extension port for up to 4 additional relay or sensor boards.

### Lightswitch, sensors, CAN header/hub, UART adapter

Additional boards in `hardware/` — see each board's own KiCad project for
details; written documentation for these is still a work in progress.

## CAN bus and MQTT protocol

See [`docs/protocol-can.md`](docs/protocol-can.md) for the byte-level CAN
wire format shared by every component, and
[`software/gateway-core/docs/protocol-mqtt.md`](software/gateway-core/docs/protocol-mqtt.md)
for the gateway's MQTT topic scheme. (Extension-board channel/button
numbering conventions used by the node firmware — e.g. which CAN
`NO`/button-id ranges map to which extension address — are documented at
the node firmware level; `docs/protocol-can.md` covers the CAN frame
format itself.)

## CI

- **CI** — `cargo fmt` / `cargo clippy` for every crate, `cargo nextest`
  with coverage and an MSRV check for the `*-core` crates.
- **Docs** — `cargo doc` for every crate, published to GitHub Pages.
- **Security audit** — [`cargo-audit`](https://github.com/rustsec/rustsec)
  against the RustSec advisory database, on every dependency change and
  once daily.
- **Renovate** — automated dependency-update PRs, once daily.

`software/legacy/` (the original C++/ESP-IDF code) is no longer built by
CI — see its own `README.md` for the ESP-IDF build steps if you need to
reproduce it.

## License

GNU General Public License v3.0 (or later) — see [LICENSE](LICENSE).

This is meant to be freely buildable, flashable, and modifiable hardware
and firmware. GPLv3 was chosen deliberately: its copyleft means anything
built on this stays open, and its Section 6 "Installation Information"
requirement means anyone who conveys a modified version as part of a
physical product also has to provide what's needed to install your own
modified firmware on it — nobody gets to lock you out of hardware you own.
All schematics, layouts, mechanics, tools, and software are provided so
you can fix things yourself rather than throwing them away.

## Contribution

Feel free to report issues or open a pull request.
