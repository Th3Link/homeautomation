# ADR 0011: Multiple gateways, mesh-aware, with on-gateway automation independent of MQTT

**Status:** Proposed — not yet implemented. Today there is exactly one
gateway, controlling all nodes, with no gateway-to-gateway communication
and no automation logic running on the gateway itself.

## Context

The single existing gateway currently controls too many nodes: it's a
single point of failure for the whole installation, and its
network-side uplink (ADR 0003) is a single point of failure for anything
that depends on the hub (openHAB/Home Assistant) reacting to a CAN event
— if the network, the broker, or the hub is down, no automation driven by
that hub runs, even for two devices sitting on the same physical CAN
segment a few centimeters apart.

The goal is measurably better resilience — the household's actual
tolerance for automation flakiness ("WAF", in the household's own
shorthand) — by reducing the blast radius of any single failure and by
letting the most latency- and reliability-sensitive automations (a button
directly driving a light, say) not depend on a network round-trip to a
hub at all.

Rust was chosen for this project in part specifically with this direction
in mind: `serde` makes defining and evolving a typed automation-rule
format (and whatever gateway-to-gateway message envelope carries it)
substantially less error-prone than the equivalent hand-rolled C
parsing/serialization would be.

## Decision (direction, not a finished design)

Move from one gateway to **three gateways**, each responsible for a
smaller subset of nodes, with two new capabilities neither the original
C++ gateway nor the current Rust port has:

- **Gateway discovery/mesh awareness** — gateways find each other (most
  naturally over the same CAN infrastructure they already bridge, given
  ADR 0004's reasoning applies just as well here — see the CAN ADR's
  "Consequences" section) and can exchange CAN-originated messages with
  each other directly, without requiring MQTT/a broker/a hub to be
  reachable at all.
- **On-gateway automation logic** — simple rules (e.g. "button event on
  gateway A drives a relay on gateway B") can be defined and executed
  entirely among gateways, independent of whether the MQTT uplink to the
  hub is currently working.

Splitting one gateway into three, by itself, is deployable independently
of the mesh/automation capability (it just needs each gateway configured
with a disjoint subset of `device_id`s to own) — the mesh/automation work
is what makes the split actually pay off in resilience terms, rather than
just adding three points of failure instead of one.

## Consequences

- This is a significant net-new scope, not a refactor of existing
  functionality — a real design pass is needed for at least: how
  gateways discover each other, what the inter-gateway message envelope
  looks like, how automation rules are expressed/stored/updated, and how
  conflicts (two gateways both trying to drive the same node) are
  avoided or resolved.
- It changes the trust model: gateways acting on each other's messages
  without a human-operated hub in the loop needs its own scrutiny (what
  happens if a gateway is compromised or malfunctioning and starts
  emitting bad automation triggers to the others).
- Nothing about `gateway-core`'s current CAN/MQTT protocol handling
  needs to change to make this possible eventually — the wire formats in
  [`../protocol-can.md`](../protocol-can.md) and
  [`../protocol-mqtt.md`](../protocol-mqtt.md) are unaffected by this
  ADR; a mesh protocol would most likely be a new, additional CAN message
  range/type, not a replacement for the existing device-bridging traffic.
