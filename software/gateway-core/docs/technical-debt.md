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

The Embassy executor, request handlers, MbedTLS, flash operations and interrupt handlers share the PRO CPU stack. A deeply nested poll can exhaust it even when its future lives in static storage. The HTTPS server places its session, HTTP server, router and selected handler in four fixed slots. NTS key exchange can use the entire area while it holds the same TLS lock. Each future must fit at compile time. Network operations sharing a slot must finish and release it before the next one starts.

Run the `https-automation` QEMU scenario after changing the router or the shape of an automation command. It writes and reads back a button, checks the boot ID and requires at least 8 KiB of untouched stack after the save and the complete edit sequence. On hardware, compare `stack_untouched` and the separate TLS pool counters under the installation's usual traffic. The per-bank layout and the QEMU measurements are in the [gateway RAM notes](ram.md).

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

## IPv4 link-local without conflict detection

The network settings offer IPv4 link-local (169.254.x.y, RFC 3927), but
only half of it: the address is derived from the MAC
(`gateway_core::network::link_local_v4`) and taken without probing. RFC
3927 § 2.2 wants ARP probes first (is the address in use?), announcements,
defending it, and a new address on a conflict; smoltcp has none of that
(no ARP probe, no conflict event), and embassy-net no raw Ethernet socket
to do it beside the stack. With 65024 addresses and a handful of
link-local devices a clash is unlikely, but it would go unnoticed.

**Fix:** IPv4 link-local in smoltcp (an `Interface` option that probes,
announces and defends, and picks a new address on a conflict), offered
upstream as a contribution if its maintainers want it; then the gateway
switches to it.

## Not yet ported from the legacy design

These aren't "bad", just genuinely unfinished relative to the original's
feature set — tracked here so they don't get lost, distinct from the
design-quality complaints above:

- **Forks of `esp-hal`, `smoltcp` and `embassy-net`.** The ESP32 MAC's
  checksum insertion is broken: it gets every second checksum or so wrong,
  and it writes the IPv4 checksums into some IPv6 frames, right over their
  addresses (so IPv6 TCP connections stalled). esp-hal 1.2.2 always switches
  it on; the fix (esp-rs/esp-hal@0eb3e53) is on esp-hal's main but in no
  release yet. smoltcp and embassy-net 0.9 lack what IPv6 first needs
  (ADR 0019). The workspace takes all three from forks
  ([`software/patches`](../../patches/README.md)); each goes once a release
  has its fixes. embassy-net never will: its main has moved to xarxa.
