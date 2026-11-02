# ADR 0021: Nodes announce themselves until the gateway acks (Discovery)

**Status:** Accepted

## Context

The gateway found the nodes on the bus by asking all of them at once: a
broadcast ping (`Available` R frame), which every node answered at the
same moment, followed by a broadcast `RequestParameter`, which every node
answered with about nine frames. With around 60 nodes that is hundreds of
frames in a burst: the nodes' transmit retries ran out and frames were
lost, so devices were missing from the list or had no name. A node that
started announced itself with two `Available` frames 5 s after boot, so a
power cut that restarted every node at once produced the same burst.

## Decision

Two new message types, in both firmwares
(`common-core/src/discovery.rs`, the protocol in `docs/protocol-can.md`):

- **`Discovery` (43)**, node → gateway: the node's name (its custom
  string, 0-8 bytes) in the payload. Sent when the node starts, when its
  name changes, and when the gateway asks.
- **`DiscoveryAck` (44)**, gateway → node, to the node's id, no payload.
- **`Discovery` as an R frame** from the gateway, to device 0 (everybody)
  or to one node: announce yourself. Sent when the gateway starts, by the
  web UI's *Scan* (`POST /bus/discover`) and the console's `discover`.

A node repeats its announcement until the ack comes:

1. the first one after a random 0-2 s, so that nodes started or asked
   together are spread out;
2. then a random 300 ms-2 s wait for the ack, for the first five tries;
3. then a random 2-5 s wait, for as long as it takes.

The random numbers come from the ESP32's generator, mixed with the device
id. The gateway learns a node and its name from the announcement itself,
acks it at once, and asks a node it hasn't seen before for its other
settings (`RequestParameter`) as before, one node at a time as their
announcements arrive.

The old ping (`Available`/`Ping` R frames) still works and is answered as
before, for nodes and tools that use it.

## Consequences

- No burst: 60 nodes asked together announce within about 2 s, each one
  frame, and only the ones the gateway doesn't know yet are asked for
  more.
- The gateway has a node's name with its first frame, so MQTT topics use
  the name right away.
- A node no longer sends `Available` after starting; its `Discovery` makes
  it known (to any gateway, which learns a node from every frame).
- A node on a bus with a gateway that doesn't know `Discovery` (the old
  C++ gateway) never gets an ack and keeps announcing every 2-5 s: one
  frame, harmless, but noise.
- Nodes with the old firmware don't answer `Discovery`; *Scan* doesn't
  find them until they are updated (the broadcast ping still does).
