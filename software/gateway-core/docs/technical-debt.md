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

## No secure boot, no flash encryption (secrets lie in flash in the clear)

Nothing on the gateway (or the nodes) is protected against someone who holds
the board: the bootloader accepts any signed-or-unsigned image, and the
flash content can be read out over the serial port. What lives in that flash
and would leak: the web and MQTT passwords, the pinned broker certificates
and — once HTTPS lands — the device's HTTPS private key. To do:

- **Encrypt the HTTPS private key** at rest, so a flash dump alone doesn't
  give it away. Cheapest first step: wrap it with a key derived from the
  chip's eFuse (or an eFuse-held key block read-protected from software),
  independent of the full flash encryption below.
- **Flash encryption** (ESP32 eFuse `FLASH_CRYPT`) for the whole `nvs`
  partition and the images, which also covers the passwords above.
- **Secure boot** (ESP32 V2, RSA-PSS signed images): only images signed with
  our key boot. The OTA paths — the gateway's own `PUT /firmware/gateway` and
  the CAN block updates of the nodes — then have to carry signed images, and
  the signing key needs a place in the release process (CI secret, never in
  the repo).

Both are one-way eFuse burns on a board, so they need a recovery/test story
(a sacrificial board first) and a decision on development builds before they
are switched on. Status: not started.

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

## One main stack for everything (gateway)

Every task's poll, MbedTLS's handshake and the flash access run on the one
main stack (~47 KB, what DRAM leaves after the statics). Rust's async code
needs much more of it than the future sizes suggest: a poll function keeps
room for every future it builds by value, and `Box::pin(f())` builds `f`'s
future on the stack before moving it to the heap. The HTTPS request path
went past 60 KB that way and wrote below the stack into the statics
without hitting esp-hal's stack guard (one word): random crashes on the
first authenticated request. It is down to ~32 KB now: large futures are
built through `heap::boxed` (in their own frame, polled through `dyn`, so
no caller keeps room for them), the certificate status is boxed, flash
reads and writes skip `esp-storage`'s 4 KiB stack buffer, and the MQTT
buffers are static.

The margin is measured, not computed: the stack is painted at boot and
`GET /system` reports `stack_untouched`. ~14 KB were left after HTTPS
requests, uploads and a firmware update over HTTPS; MQTT over TLS (RSA
signature check on the stack) is not measured yet. A new large future in
a handler or task shows up there first.

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

- **Patched copy of `esp-hal`.** The ESP32 MAC's checksum insertion is
  broken: it gets every second checksum or so wrong, and it writes the IPv4
  checksums into some IPv6 frames, right over their addresses (so IPv6 TCP
  connections stalled). esp-hal 1.2.2 always switches it on; the fix
  (esp-rs/esp-hal@0eb3e53) is on esp-hal's main but in no release yet, so
  the workspace builds a patched copy of 1.2.2
  ([`software/patches`](../../patches/README.md)). It goes once a release has
  the fix.
- **CAN-bus MQTT logging passthrough** (`canbus/log/0x<hex>`, toggled by
  the `mqtt_logging` RPC verb in the original) is speced in
  [`protocol-mqtt.md`](protocol-mqtt.md#canbuslog0xhex-planned-in-the-original-not-currently-wired-up)
  but not implemented — there's no code in `gateway-hardware` mirroring
  every sent/received CAN frame to MQTT yet.
