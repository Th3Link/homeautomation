# ADR 0003: MQTT as the transport to openHAB/Home Assistant

**Status:** Accepted

## Context

The gateway's job on the network side is to expose CAN-bus device state
(relay/rollershutter positions, button/presence events, environmental
sensor readings) and accept commands for those devices, to whatever home
automation hub the household runs — in practice, openHAB or Home
Assistant. Both have mature, first-class MQTT integrations (auto-discovery
support, well-understood retained-message and QoS semantics, broad
community familiarity), and MQTT's publish/subscribe model maps directly
onto "many independent devices publish state changes, a hub subscribes to
all of them and publishes commands back" without needing the gateway to
know anything about which hub, or how many clients, are actually
listening.

A broker (Mosquitto, EMQX, the hub's built-in broker, ...) is also
already a piece of infrastructure most home automation setups run
regardless of this project, so the gateway doesn't have to provide or
manage one itself.

## Decision

Keep MQTT as the one supported uplink transport to the home automation
hub, unchanged from the original design: the gateway is an MQTT client
(not a broker), publishing CAN-bus state and subscribing to command
topics under a `canbus/...` tree (see
[`../protocol-mqtt.md`](../protocol-mqtt.md) for the exact topic scheme).
No alternative transport (a REST push API to the hub, a vendor-specific
integration, etc.) is in scope.

## Consequences

- The gateway depends on network connectivity to a broker for any
  hub-facing functionality — if the network or the broker is down, no
  state reaches the hub and no commands can be received through it. This
  is the exact problem ADR 0011 (planned) addresses for the CAN side, by
  letting gateways coordinate directly over the bus without requiring
  MQTT to be reachable at all for basic inter-gateway automation.
- Every device-state change and command still has to round-trip through
  whatever network path connects the gateway to the broker (see ADR
  0002) — there's no local/direct path from a hub to a CAN device that
  bypasses MQTT today.
- Wire-level choice of MQTT protocol version (3.1.1 vs. 5.0) is a
  separate decision — see ADR 0007.
