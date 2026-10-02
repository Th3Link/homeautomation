# Technical debt

Known problems and unfinished work, as of the initial Rust port. Each
item below either has, or points to, the ADR tracking its planned
resolution — this document is the "what's wrong and why it matters" side;
the ADRs are the "what we've decided to do about it" side.

## CAN-bus OTA is unreliable

Pushing firmware to a node over the CAN bus fails often in practice. The
current implementation (both in the original C++ and this port, which
faithfully reproduces it — see
[`protocol-can.md`](../../../docs/protocol-can.md#ota-flashstartflashwritecomplete))
streams the image as a flat sequence of `FlashWrite` frames with no
per-chunk checksum, no sequence numbering, and no acknowledgement:

- A dropped frame (bus contention from other live traffic, electrical
  noise, a busy receiver) is invisible to the sender — nothing detects
  it, nothing retransmits it. The transfer just continues, and the
  target's flash ends up with a gap.
- A corrupted-but-received frame is written to flash as-is — there's no
  per-chunk checksum to catch it before it's committed.
- Pacing is a fixed delay (`update_delay_ms` between frames, plus a pause
  every 4096 bytes) tuned by feel, not backed by any actual flow-control
  signal from the receiver.

**Planned fix:** [ADR 0010](../../../docs/adr/0010-can-ota-rework-planned.md) — rework
into a chunked protocol with per-chunk sequence numbers, checksums,
write-before-ack, and sender-side retransmission on a missing ack or
timeout. Not yet designed in detail or implemented.

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

- **The web configuration server** (`/state.json`, `/control.json` itself,
  `/bridge_config.json`, `/update/data`, and the static config-page UI)
  doesn't exist in `gateway-hardware` yet. The CAN↔MQTT bridge, the
  commissioning console, self-OTA, and CAN-OTA orchestration are all
  implemented and wired into `main.rs`; the HTTP layer connecting them to
  a browser is not. (`picoserve` was selected as the library for this and
  is already a dependency, but no routes are implemented.)
- **CAN-bus MQTT logging passthrough** (`canbus/log/0x<hex>`, toggled by
  the `mqtt_logging` RPC verb in the original) is speced in
  [`protocol-mqtt.md`](protocol-mqtt.md#canbuslog0xhex-planned-in-the-original-not-currently-wired-up)
  but not implemented — there's no code in `gateway-hardware` mirroring
  every sent/received CAN frame to MQTT yet.
