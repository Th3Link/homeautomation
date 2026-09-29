# ADR 0004: CAN bus for inter-component communication

**Status:** Accepted

## Context

Each physical device in the home-automation installation (relay board,
rollershutter controller, button panel, presence/environmental sensor
node) needs a reliable, low-power, low-cost link back to a gateway. Common
alternatives considered for this role, implicitly, by what the original
project already built on:

- **A wireless mesh (Zigbee, Z-Wave, ESP-NOW, ...)** — adds a radio to
  every single node, adds pairing/association complexity, and trades a
  physical-layer problem (wiring) for an RF-environment problem
  (interference, range, battery life for anything not mains-powered).
- **A second wired bus (RS-485/Modbus, Ethernet to every node, ...)** —
  more expensive per node (transceiver + often a full MCU-with-Ethernet
  vs. a CAN transceiver), and most of these home-automation nodes
  (relay/rollershutter/button boards) don't need anything close to
  Ethernet's bandwidth.
- **CAN bus** — a differential two-wire multi-drop bus, natively
  multi-master and collision-handling, cheap transceivers, tolerant of a
  noisy electrical environment (shares conduit with mains wiring in a lot
  of these installs), and the microcontrollers used for these simple
  nodes (STM32, ESP32) have built-in CAN/TWAI controllers, so per-node
  cost is close to zero beyond the transceiver chip.

## Decision

Keep CAN as the bus connecting the gateway to every relay/rollershutter/
button/sensor node, unchanged from the original design — see
[`../protocol-can.md`](../protocol-can.md) for the wire format. The
gateway runs its CAN transceiver in promiscuous mode (see ADR 0009) and
bridges bus traffic to/from MQTT.

## Consequences

- Every node needs a CAN transceiver and physical bus wiring run to it —
  this is a real installation cost, but one already paid by every
  existing deployment; changing bus technology now would mean rewiring.
- CAN's ~1 Mbit/s practical ceiling (and lower, depending on bus length/
  node count — see [`../protocol-can.md`](../protocol-can.md) for the
  bitrate options actually in use) is fine for this project's message
  rates (state changes, button events, periodic sensor readings) but
  would not be fine for anything bulkier — firmware OTA over this same
  bus is the one place that ceiling actually bites, see ADR 0010.
- Because CAN is multi-drop and multi-master by design, it's also the
  natural path for gateway-to-gateway communication once there's more
  than one gateway on (or bridging) the same bus segment — see ADR 0011.
  This wasn't the original reason CAN was chosen, but it's a real
  consequence of having chosen it.
