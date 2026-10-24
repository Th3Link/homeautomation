# homeautomation

A CAN-bus home-automation system: relay/rollershutter/light/button/sensor
nodes on a shared CAN bus, bridged to MQTT (and from there to openHAB or
Home Assistant) by a gateway. This repository holds the hardware (KiCad),
mechanics (3D-printable enclosures), and firmware (Rust, on ESP32) for the
whole system, plus the documentation tying it together.

## Components

- **The gateway** (`software/gateway-core` + `software/gateway-hardware`)
  — bridges the CAN bus to MQTT, over wired Ethernet
  (DHCPv4 + SLAAC) (see `software/gateway-core/docs/adr/0013-ethernet-only-no-wifi.md`).
  Start at
  [`software/gateway-core/docs/requirements.md`](../software/gateway-core/docs/requirements.md).
- **CAN nodes** (`software/cancomponents-core` +
  `software/cancomponents-hardware`) — the relay, rollershutter, and
  button/light firmware that actually lives on the bus. See
  [`software/cancomponents-core/docs/`](../software/cancomponents-core/docs/index.md).
- **`software/common-core`/`software/common-hardware`** — code shared by
  both firmware lines: the CAN wire-format types, and ESP32
  hardware-support boilerplate (flash access, console-aware logging,
  config storage). See [ADR 0012](adr/0012-shared-common-crates.md).
- **`hardware/`** — KiCad PCB projects for every board in the system.
- **`mechanics/`** — OpenSCAD/STL enclosures and mounts.
- **`software/tools/mqtt-log-reader/`** — a small Python tool for reading
  back the gateway's MQTT log/debug topics.
- **`software/legacy/`** — the original C++/ESP-IDF implementation
  (`mqtt_gateway`, `cancomponents`, and their shared `library`), kept for
  reference. No longer built by CI; superseded by the Rust crates above.

## Why CAN, why MQTT

See [ADR 0004](adr/0004-can-bus-inter-component-communication.md) (CAN as
the inter-component bus) and [ADR 0003](adr/0003-mqtt-transport-to-home-automation-hub.md)
(MQTT as the uplink to a home-automation hub) for the reasoning — cheaper,
simpler, and more energy-efficient nodes than a WiFi-per-node design, with
one gateway translating to the MQTT-speaking world.

## Finding your way around

- [`requirements.md`](requirements.md) — what the system does, system-wide,
  linking to each component's own requirements.
- [`adr/index.md`](adr/index.md) — every architecture decision record,
  workspace-wide and component-scoped, in one numbered index.
- [`protocol-can.md`](protocol-can.md) — the CAN wire format shared by
  every component on the bus.
- Component-scoped protocol/technical-debt docs (e.g. the gateway's MQTT
  topic reference) live next to their crate — see the component list
  above.

This documentation set is built into `cargo doc` output for every crate
(via `#[doc = include_str!(...)]` marker modules — see each crate's
`docs` module) and published to GitHub Pages on every push to `main`.
