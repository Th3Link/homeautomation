# ADR 0019: IPv6 first, with SLAAC and DHCPv6

**Status:** Accepted

## Context

The gateway ran dual-stack in name only. IPv4 came from DHCPv4; IPv6 had
SLAAC from `embassy-net`/smoltcp, which kept one address (the last prefix of
a Router Advertisement), replaced the routers' default routes with one that
never expired, ignored RDNSS and the M/O flags, and had no DHCPv6. Every
name was looked up with an A query. The ESP32's MAC wrote IPv4 checksums
into some IPv6 frames, so IPv6 TCP connections stalled. On a network that
hands out addresses or DNS servers only over DHCPv6, or has no IPv4 at all,
the gateway was unreachable or could not reach its broker.

## Decision

**IPv6 is the first choice, IPv4 stays.** Both are configured without
setup; neither depends on the other.

- **Addresses.** The link-local address, one SLAAC address per prefix the
  routers announce (with their lifetimes), and a DHCPv6 address when the
  routers set `M`. IPv4 from DHCPv4 as before.
- **DHCPv6** (`dhcpeav6-client` from github.com/Th3Link/dhcpea, a sans-IO
  client without allocator, written for this after mozim's flow): the routers' flags decide. `M`:
  an address (IA_NA, as a /128) plus configuration, renewed at T1/T2 and
  given back before every restart. Only `O`: configuration only
  (Information-request). Neither: off. Its DNS servers join the others;
  its NTP server is the default for SNTP.
- **DNS servers** from DHCPv4, RDNSS (RFC 8106) and DHCPv6, all used.
- **Names are looked up AAAA first** (`gateway-hardware/src/resolve.rs`) when
  the gateway has an IPv6 address beyond link-local, A first when it has
  only IPv4; a family it has no address in isn't asked for. The addresses
  are tried in that order, 10 s each (MQTT, EST), or until one answers
  (SNTP).
- **IPv6 literals** in brackets wherever a URL is configured:
  `mqtt://[fd00::1]:1883`, `https://[fd00::1]:8443` for EST.
- **IPv6-only works:** no part of the gateway waits for DHCPv4.

**Forks** (by git revision, `software/patches/README.md`) of esp-hal (no TX checksum
insertion on the ESP32), smoltcp (every prefix of an RA, the routers' M/O
flags and RDNSS, the SLAAC default route next to IPv4's) and embassy-net
(every address kept, RDNSS as DNS, `set_dhcpv6_lease`), until upstream has
the fixes.

**What it shows.** The console's `network`, `GET /system` (`network`) and
the UI list every address with its origin, the gateways, the DNS servers
with theirs, and what DHCPv6 does.

## Consequences

- One more socket (DHCPv6) and ~2.5 KB of heap for the client.
- With DHCPv6 the gateway may have three IPv6 addresses (link-local, SLAAC,
  DHCPv6): any of them reaches it; the DHCPv6 one is stable as long as the
  server keeps the lease, the SLAAC one as long as the MAC and prefix stay.
- Not supported: temporary addresses (RFC 8981), DHCPv6 prefix delegation,
  DNSSL from Router Advertisements, Confirm after a link change (Rebind
  instead), more than one DHCPv6 address, mDNS.
- An EST server addressed by an IPv6 (or IPv4) address must have it as a
  SAN IP address in its certificate; MbedTLS also sends it as SNI.
- The patched versions have to be dropped (or rebased) as upstream
  releases the fixes; `software/patches/README.md` tracks them.
