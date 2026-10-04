# ADR 0013: Wired Ethernet is the only network path — drop WiFi

**Status:** Accepted

Supersedes [ADR 0002](0002-ethernet-primary-wifi-fallback.md) (Ethernet
first, WiFi as fallback) and [ADR 0006](0006-esp-radio-over-esp-wifi.md)
(which `WiFi` crate to use — moot now).

## Context

ADR 0002 kept WiFi as a fallback for installations without a wired drop.
In practice the gateway is always installed hardwired next to the CAN bus
wiring, and the WiFi path is never used. Keeping it nevertheless cost a
lot, measured on the release build before and after removing it:

| | with WiFi | Ethernet only |
|---|---|---|
| Flash image | 898 KiB | 490 KiB |
| Static DRAM (`.data` + `.bss`) | 151 KiB | 54 KiB |
| DRAM left over as stack budget | 41 KiB | 138 KiB |

(The Ethernet-only column includes the IPv6/SLAAC stack added alongside
this change, about 80 KiB of flash; without it the image is 410 KiB.)

Most of that is `esp-radio` itself (WiFi code and read-only data) and the
72 KiB heap it forced into the firmware — the only heap in the whole
project, which also broke the otherwise "no heap allocation" design. It
also required the "never both at once" rule from ADR 0002 (the RMII
reference clock is APLL-derived, which esp-hal documents as unstable
while WiFi is active), a boot-time link timeout, a setup access point with
a hardcoded SSID/password and a static `192.168.4.1` address (there is no
DHCP *server* in `embassy-net` to hand clients an address), and three
config keys plus three console commands.

## Decision

Remove WiFi from `gateway-hardware` entirely: the `wifi` module, the
`esp-radio`/`esp-alloc` dependencies (and with them the heap and
`build-std`'s `alloc`), the WiFi config fields and keys, and the
`wifi-*` console commands. In `gateway-core` the `WifiMode` type,
`resolve_wifi_credentials`, and the `wifi_*` fields of `GatewayConfig`
are gone; config keys 1–3 (the former WiFi mode/SSID/password) stay
reserved and are not reused.

Ethernet is brought up unconditionally, and the network stack runs
dual-stack: a DHCPv4 client announcing the configured hostname (DHCP
option 12), plus IPv6 SLAAC. There is no
link-up wait or timeout: the EMAC driver reports link state to
`embassy-net`, which starts DHCP when the cable comes up and redoes it
after a drop, so a cable plugged in late or swapped at runtime just works.

First-time setup therefore needs no network configuration at all: a
freshly flashed gateway takes its addresses from the LAN (DHCPv4, SLAAC)
and is reachable by its hostname; the MQTT broker and credentials are set over
the serial console.

## Consequences

- A gateway needs a wired Ethernet drop. An installation without one needs
  an external WiFi-to-Ethernet bridge.
- The "no heap allocation" requirement now holds for the whole project
  without exception.
- ADR 0002's boot-time fail-over limitation (cable pulled at runtime only
  recovers after a reboot) disappears along with it: link loss and
  recovery are handled live.
- IPv6 addressing is SLAAC only: `embassy-net`/`smoltcp` have no DHCPv6
  client. The MQTT broker may be given as a hostname (A, then AAAA
  lookup) or a bracketed IPv6 literal, `mqtt://[fd00::1]:1883`.
- The APLL/GPIO17 clock wiring facts from ADR 0002 still apply to the
  board, they just no longer constrain anything in software.
- The original's persisted WiFi settings have no place in the new config
  schema; like the rest of the NVS data they're not migrated (the flash
  layout isn't compatible with the original's NVS anyway).
