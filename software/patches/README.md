# Patched upstream crates

Crates.io releases with fixes the gateway needs before upstream releases
them, as forks on GitHub. The workspace takes them through
`[patch.crates-io]` in the root `Cargo.toml`, by git revision.

## Forks

| Crate | Fork, branch | Base | Fixes | Upstream |
|---|---|---|---|---|
| `smoltcp` | [Th3Link/smoltcp](https://github.com/Th3Link/smoltcp), `v0.13.1-gateway` | 0.13.1 | SLAAC default route next to IPv4 routes (smoltcp-rs/smoltcp#1206, taken over); every prefix of a Router Advertisement (`wire::NdiscOptionIter`, issue #1165); the routers' M/O flags and RDNSS servers (`Interface::slaac_router_flags`, `Interface::slaac_dns_servers`); no endless polling after three unanswered Router Solicitations | PR branches on 0.14 in the fork: `slaac-every-prefix`, `slaac-router-flags-rdnss` (on top of the former), `slaac-solicitation-poll` |
| `esp-hal` | [Th3Link/esp-hal](https://github.com/Th3Link/esp-hal), `esp-hal-v1.2.2-gateway` | 1.2.2 | ESP32: no TX checksum insertion by the MAC; the network stack computes the checksums. A second commit drops the path dependencies on the sibling crates (they come from crates.io) | Fixed on main in esp-rs/esp-hal@0eb3e53 (option `ethernet.tx_checksum_offload`), not released yet; issue esp-rs/esp-hal#6173 |
| `embassy-net` | [Th3Link/embassy](https://github.com/Th3Link/embassy), `embassy-net-v0.9.1-gateway` | 0.9.1 | All SLAAC addresses kept (one per prefix), link-local first; SLAAC's routes left to smoltcp; RDNSS servers as DNS servers; `Stack::ipv6_addresses`, `Stack::ipv6_router_flags`, `Stack::wait_ipv6_router_flags_change`, `Stack::set_dhcpv6_lease`. A second commit drops the path dependencies on the sibling crates (they come from crates.io) | Needs the smoltcp fork. No PR possible: embassy-net's `main` moved from smoltcp to xarxa, and there is no 0.9 maintenance branch; xarxa already has multiple prefixes and the M/O flags, RDNSS is on its to-do list |
| `embedded-tls` | [Th3Link/embedded-tls](https://github.com/Th3Link/embedded-tls), `embedded-tls-v0.19.0-gateway` | 0.19.0 (865e1fd) | The client required no server authentication before the server's Finished: without Certificate and CertificateVerify an on-path attacker completed the handshake, whatever the verifier (pinned key, CAs). Backport of upstream's fix. And the TLS exporter, `export_keying_material` (RFC 8446 7.5), for the NTS keys (ADR 0023), tested against rustls | The fix is upstream since 999157a (embassy-rs/embedded-tls#215), not released yet; the exporter is to be proposed upstream |

A fork's gateway branch starts at the release tag, so it differs from the
release by the fixes alone. A new revision goes into the root
`Cargo.toml`.

Building the gateway shows a dozen warnings from smoltcp: they are
upstream's, for the feature set the gateway uses, and don't fail `clippy
-D warnings` (it is no workspace member). The smoltcp fork has its own
tests (`cargo test --lib`, about 670).

## Updating

When a new release comes out: compare its changelog with the fixes above.
If it has all of them, delete the fork's `[patch.crates-io]` line. If not,
rebase the remaining fixes onto the new release's tag in a new gateway
branch (with the commit that drops the path dependencies, for esp-hal and
embassy-net) and point the root `Cargo.toml` at it. Then update the table.
