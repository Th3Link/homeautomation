# Technical debt

Known problems and unfinished work, as of the initial Rust port. Each
item below either has, or points to, the ADR tracking its planned
resolution — this document is the "what's wrong and why it matters" side;
the ADRs are the "what we've decided to do about it" side.

## CAN-bus OTA (legacy streaming protocol)

The original streaming OTA (`FlashStart`/`FlashWrite`/…, faithfully reproduced
in `gateway_core::can_ota` and used by the gateway for the old STM32 nodes and
as a fallback) is unreliable: no acknowledgement, no checksum, pacing by fixed
delays, and a receiver that writes flash inline loses frames (see
[ADR 0017](adr/0017-block-ota.md)). The `cancomponents` nodes no longer speak
it: they take the block protocol, which acknowledges and repeats blocks. What
is left of the problem is only the legacy path of the gateway:

- updating the old STM32 nodes, and updating **all devices of a type at once**
  (a broadcast cannot be acknowledged by one sender), still stream without
  acknowledgement;
- a node whose firmware predates the block protocol is detected by a missing
  answer to `Begin` and updated the old way once; after that it takes blocks.

## The `/control.json` RPC API is not good

Carried over from the original almost mechanically (see "Legacy parity"
in [`requirements.md`](requirements.md)): one endpoint, a `command`
string field dispatching to one of 14 unrelated verbs, each with its own
ad hoc set of expected body fields, four different device-targeting modes
(`can_all`/`can_by_type`/`can_selected`/`can_by_uid`, each expecting a
differently-shaped `commandId`), and no consistent error reporting (an
unresolvable target, a malformed body, or a value that doesn't fit a
field's expected type are all silently dropped with no response body
indicating what went wrong).

This was ported as-is because the initial goal was exact behavioral
parity with the legacy gateway, not an improved API — but it was never a
well-designed API to begin with, and porting it faithfully didn't fix
that. No replacement design exists yet. Whatever comes next should at
minimum: separate concerns instead of one verb-dispatch endpoint, have
one consistent targeting mechanism instead of four, and report errors
instead of silently dropping malformed requests.

## Single gateway, too many nodes, no gateway-to-gateway coordination

The one gateway that exists today controls every node in the
installation, and depends entirely on the MQTT uplink (network + broker +
hub) being reachable for anything driven by hub-side automation — a
failure anywhere in that chain stops automation for the *entire*
installation, including for two devices sitting on the same physical CAN
segment.

**Planned fix:** [ADR 0011](../../../docs/adr/0011-gateway-mesh-planned.md) — split
into three gateways, each owning fewer nodes, with gateway discovery and
direct CAN-originated message exchange between them, plus on-gateway
automation logic that doesn't depend on the MQTT uplink at all. This is
a substantial net-new capability, not a refactor, and is not designed in
detail yet (discovery mechanism, inter-gateway message envelope,
automation rule format, and conflict handling are all still open).

## Not yet ported from the legacy design

These aren't "bad", just genuinely unfinished relative to the original's
feature set — tracked here so they don't get lost, distinct from the
design-quality complaints above:

- **HTTPS.** The web server (REST API and UI, [`api-rest.md`](api-rest.md))
  speaks plain HTTP only, so the Basic Auth password crosses the LAN in the
  clear. `embedded-tls` is client-only; a server needs another TLS library
  (`mbedtls-rs` is the candidate, not tried on this chip and `esp-hal`) or an
  own server handshake. picoserve takes any socket that implements its
  `io::Socket` trait, so a TLS socket can slot in.
- **Ethernet checksums.** The `esp-hal` Ethernet driver leaves the TCP/IP
  checksums to the MAC, and the MAC gets them wrong for much of the traffic:
  responses over a few hundred bytes never arrived. The stack computes them
  itself now (`SoftwareChecksums` in `gateway-hardware/src/ethernet.rs`); the
  cause in the driver or the silicon wasn't found.
- **CAN-bus MQTT logging passthrough** (`canbus/log/0x<hex>`, toggled by
  the `mqtt_logging` RPC verb in the original) is speced in
  [`protocol-mqtt.md`](protocol-mqtt.md#canbuslog0xhex-planned-in-the-original-not-currently-wired-up)
  but not implemented — there's no code in `gateway-hardware` mirroring
  every sent/received CAN frame to MQTT yet.
