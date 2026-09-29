# Requirements

## Context

This gateway bridges a CAN bus of home-automation nodes (relays,
rollershutters, button panels, presence/environmental sensors) to MQTT,
for consumption by a home automation hub (openHAB or Home Assistant — see
[ADR 0003](adr/0003-mqtt-transport-to-home-automation-hub.md)). It also
provides a local web UI for configuration/status and a serial
commissioning console, and can push firmware updates both to itself and
to CAN nodes.

The Rust rewrite's initial goal was exact functional parity with an
existing C++/ESP-IDF implementation — see "Legacy parity" below — as the
foundation for the further changes and new features described under
"Planned" that motivated doing the rewrite in the first place.

## Legacy parity (implemented)

Everything in this section reproduces the original C++ gateway's actual
*running* behavior (not code that was present but dead/never built — see
"Explicitly out of scope" below), including its quirks, unless a
deliberate deviation is called out (and cross-referenced to the ADR that
made it) — see [`protocol-can.md`](protocol-can.md) and
[`protocol-mqtt.md`](protocol-mqtt.md) for the byte- and topic-level
specification this section summarizes.

- **CAN↔MQTT bridging** for every live device/message class: relay and
  rollershutter state and commands, lamp and nightlight commands, button
  and presence (PIR) events, environmental sensor readings (temperature,
  pressure, humidity, CO₂, VOC, air quality, ambient light), and a raw
  CAN debug passthrough topic.
- **CAN node registry**: passively tracks every device seen on the bus
  (up to 100 entries), resolving a friendly custom name or a raw hex id
  to the device's CAN address for command targeting, and requesting
  parameters from a device the first time it's seen.
- **`/control.json` RPC API**: the same verb set as the original
  (`save_config`, `relais`, `rollershutter`, `lamp`, `mqtt_logging`,
  `save` device-provisioning, `refresh`, `ping`, `restart`, `legacy_mode`,
  `silence_on`/`silence_off`, `update_prepare`, `update_complete`), with
  the same `can_all`/`can_by_type`/`can_selected`/`can_by_uid` targeting
  model. Known to need rework — see
  [`technical-debt.md`](technical-debt.md#the-controljson-rpc-api-is-not-good).
- **Consolidated configuration**: WiFi/Ethernet, MQTT broker, web auth,
  this gateway's own CAN identity, and CAN-OTA pacing — one typed schema,
  unlike the original's ~17 independently-defaulted NVS entries scattered
  across modules.
- **Self firmware OTA** over the web UI's upload endpoint.
- **CAN-bus firmware OTA**, pushing an image to one node, every node of a
  type, or a selected set. Known to be unreliable in its current (legacy,
  faithfully reproduced) form — see
  [`technical-debt.md`](technical-debt.md#can-bus-ota-is-unreliable) and
  [ADR 0010](adr/0010-can-ota-rework-planned.md).
- **Serial commissioning console**: device identity (id/type/hardware
  revision/custom string/CAN bitrate), WiFi/MQTT/web-auth bootstrap
  fields, and basic bring-up commands (ping, restart).
- **Web config UI**, authenticated — see
  [ADR 0008](adr/0008-mandatory-web-authentication.md) for the one
  deliberate behavioral difference from the original here (mandatory
  auth on every endpoint, closing two gaps the original had).

## Non-functional requirements

- **Host-testable core logic.** Wire format, MQTT topic translation, RPC
  encoding, and the CAN-OTA orchestration must be exercisable with
  `cargo test` on a normal machine, with no hardware, emulator, or esp
  toolchain involved — see
  [ADR 0001](adr/0001-workspace-split-core-hardware.md).
- **No heap allocation** outside of what a single third-party dependency
  (WiFi, via `esp-radio`) requires internally — everything else in
  `gateway-hardware`, and all of `gateway-core`, uses `heapless`/static
  allocation.
- **Documented, typed configuration** with one schema and one set of
  defaults, replacing the original's per-module NVS scatter.
- **No silent security regressions** relative to the original — see
  [ADR 0008](adr/0008-mandatory-web-authentication.md).

## Planned (not yet implemented)

These are the concrete reasons the rewrite was undertaken in the first
place, beyond parity — captured here so they don't get lost, even though
none of them are built yet:

- **Reliable CAN-bus OTA.** Chunked, checksummed, sequenced, acknowledged,
  retransmitting — see [ADR 0010](adr/0010-can-ota-rework-planned.md) and
  [`technical-debt.md`](technical-debt.md#can-bus-ota-is-unreliable).
- **A better RPC API** than `/control.json` in its current shape — see
  [`technical-debt.md`](technical-debt.md#the-controljson-rpc-api-is-not-good).
  No replacement design chosen yet.
- **Multiple, smaller gateways** (three, each owning fewer nodes than
  today's single gateway) that can **discover each other and exchange
  CAN-originated messages directly**, without requiring the MQTT uplink
  to a hub to be reachable, plus **automation logic running on the
  gateways themselves** — so basic automations keep working independent
  of network/broker/hub availability. See
  [ADR 0011](adr/0011-gateway-mesh-planned.md). This is the headline
  reason Rust (and `serde` specifically, for a typed rule/message format)
  was chosen for this rewrite.

## Explicitly out of scope

Established during the initial port, and still applicable to anything
added going forward unless a specific decision reverses it:

- **Automation/rule logic that runs on *this* gateway today.** The
  original's `Action`/`ActionLight`/`ActionPresence` classes suggest an
  automation engine was once intended, but that code never compiled and
  was never wired into the build — no rule engine exists in the legacy
  behavior this port targets. (This is superseded by "Planned" above for
  *future* work — it's listed here because it's not part of what "legacy
  parity" means.)
- **Node firmware.** Sensor/button/relay driver code for the boards on
  the *other* end of the CAN bus is a separate concern (see
  `cancomponent-rs`, reference-only in this repository) — this project
  only implements the gateway side of the bus.
- **A second, generic CAN↔MQTT translation framework.** The original
  repository contains an abandoned, never-integrated attempt at one
  (`mqtt_gateway/main/bridge/*`) alongside the hand-written translation
  that's actually live — only the latter's behavior is in scope.
