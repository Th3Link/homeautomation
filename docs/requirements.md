# Requirements (workspace-wide)

This is the system-level view — what the whole home-automation system
(hardware, mechanics, and firmware together) needs to do. Each component
has its own, more detailed requirements document; this one only covers
what's genuinely cross-cutting.

## System-level requirements

- **CAN as the inter-component bus.** Every node (relay, rollershutter,
  button panel, sensor) and the gateway communicate over a shared CAN bus
  — cheaper, simpler, and more energy-efficient than giving every node its
  own WiFi radio. See [ADR 0004](adr/0004-can-bus-inter-component-communication.md).
- **One gateway per bus segment bridges CAN to MQTT**, for consumption by
  a home-automation hub (openHAB or Home Assistant). See
  [ADR 0003](adr/0003-mqtt-transport-to-home-automation-hub.md).
- **A shared wire-format contract.** The gateway and every node must agree
  on the CAN ID layout, message-type discriminants, and payload formats —
  see [`protocol-can.md`](protocol-can.md), and
  [ADR 0012](adr/0012-shared-common-crates.md) for how that agreement is
  enforced in code (a shared `common-core` crate, not independently
  maintained copies).
- **Firmware updates travel over the same CAN bus** the nodes already use
  — no second update channel. The current implementation is unreliable;
  see [ADR 0010](adr/0010-can-ota-rework-planned.md) for the planned fix.

## Planned direction

Today, one gateway typically controls more nodes than is ideal — a single
point of failure for the whole bus segment it serves, and, in practice, a
household-relations problem when MQTT/the hub goes down and *everything*
stops responding. The planned direction is multiple, smaller gateways
(fewer nodes each) that can discover each other and exchange CAN messages
directly, with simple automation logic runnable on the gateways
themselves — independent of any MQTT/hub uplink. See
[ADR 0011](adr/0011-gateway-mesh-planned.md).

## Component requirements

- [`software/gateway-core/docs/requirements.md`](../software/gateway-core/docs/requirements.md)
  — the gateway's own requirements (legacy parity, the `/control.json` API,
  the web UI, planned changes).
- `software/cancomponents-core/docs/` — not yet written; see
  [`software/cancomponents-core/docs/index.md`](../software/cancomponents-core/docs/index.md).

## Explicitly out of scope

- Anything in `software/legacy/` (the original C++/ESP-IDF implementation)
  is kept for reference only and isn't built or maintained going forward.
