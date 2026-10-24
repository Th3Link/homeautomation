# ADR 0002: Ethernet is the primary network path, WiFi is a fallback, never both at once

**Status:** Superseded by [ADR 0013](0013-ethernet-only-no-wifi.md)

## Context

Gateways are normally installed hardwired, next to the CAN bus wiring
they're bridging — a wired Ethernet drop is the common case, not the
exception. WiFi is a secondary convenience for the minority of
installations without one. The original C++ (`LAN.cpp`/`WiFi.cpp`/
`Network.cpp`) reflects this: it brings up both an Ethernet (`esp_eth_*`,
RMII to an external PHY) and a WiFi (`esp_wifi_*`) network interface
unconditionally, as two independent `esp_netif` instances.

Reimplementing this straightforwardly surfaced a constraint the original
doesn't appear to account for. The gateway board wires the ESP32's RMII
reference clock in `EMAC_CLK_OUT` mode (`clock_config.rmii.clock_mode =
EMAC_CLK_OUT`, output on GPIO17) — the ESP32 generates the 50 MHz RMII
clock itself via its APLL, rather than the PHY chip supplying it. esp-hal's
own documentation for this clock source states plainly that the APLL "is
unstable when Wi-Fi is active" and that esp-hal does no arbitration
between the two subsystems sharing it — running both at once risks silent
corruption, not just a resource-contention error that would be easy to
notice and diagnose.

## Decision

Ethernet and WiFi are never brought up at the same time. At boot, the
gateway:

1. Resets the PHY and brings up the EMAC/RMII interface.
2. Waits up to a fixed timeout (currently 5s) for link-up.
3. If the link comes up, it initializes `embassy-net` over Ethernet and
   stops there — WiFi is never touched for the rest of that boot.
4. If no link comes up within the timeout, it falls back to bringing up
   WiFi (station mode, retrying, then an access-point fallback) per the
   configured `wifi_mode`, exactly as before.

This is decided fresh on every boot (not persisted) — unplugging Ethernet
and rebooting falls back to WiFi automatically; plugging it back in and
rebooting prefers it again.

## Consequences

- A gateway with both a cable and a configured WiFi network will always
  end up on Ethernet, silently ignoring the WiFi config, for as long as
  the cable stays connected — this is intentional, not a bug to fix later.
- Unplugging the Ethernet cable while running does **not** trigger an
  automatic fail-over to WiFi mid-session; fail-over is decided once, at
  boot. A cable pulled at runtime currently just drops connectivity until
  the next reboot. Revisit if this becomes a real operational problem.
- If a future board revision sources the RMII clock from the PHY's own
  oscillator instead (`ExternalRefClock` rather than `ApllClock` in
  esp-hal's terms), this whole constraint goes away and Ethernet+WiFi
  could run concurrently — but that's a hardware change, not a software
  one, and isn't assumed here.
- The GPIO used for the APLL clock output (GPIO17, `EMAC_CLK_OUT_180`) is
  a board wiring fact read directly from the original `LAN.cpp`, not a
  free software choice; GPIO16 (`EMAC_CLK_OUT`, non-inverted) is the only
  other wired option on this chip, and switching to it is a one-line pin
  change in `gateway-hardware`.
