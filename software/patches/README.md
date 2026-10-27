# Patched upstream crates

Copies of crates.io releases with fixes the gateway needs before upstream
releases them. The workspace uses them through `[patch.crates-io]` in the
root `Cargo.toml`; they are excluded from the workspace, so `cargo` treats
each as a package of its own.

Each copy is committed unchanged first (the crates.io package as it is in
`~/.cargo/registry`, without its `Cargo.lock`), and the fixes come as
separate commits on top: `git log -p -- software/patches/<crate>` shows
exactly what differs from the release. A copy goes as soon as a release has
all of its fixes.

| Crate | Base | Fix | Upstream |
|---|---|---|---|
| `esp-hal` | 1.2.2 | ESP32: no TX checksum insertion by the MAC; the network stack computes the checksums | Fixed on main in esp-rs/esp-hal@0eb3e53 (option `ethernet.tx_checksum_offload`), not released yet; issue esp-rs/esp-hal#6173 |
| `smoltcp` | 0.13.1 | SLAAC default route kept next to IPv4 routes | smoltcp-rs/smoltcp#1206 (open, taken over as is), issue #1191 |
| `smoltcp` | 0.13.1 | SLAAC configures every prefix of a Router Advertisement, not just the last (`wire::NdiscOptionIter`) | Issue smoltcp-rs/smoltcp#1165; no PR yet |
| `smoltcp` | 0.13.1 | Routers' M/O flags and RDNSS servers (RFC 8106): `Interface::slaac_router_flags`, `Interface::slaac_dns_servers` | No PR yet |
| `smoltcp` | 0.13.1 | SLAAC no longer polls without end after three unanswered Router Solicitations | No issue or PR yet |
| `embassy-net` | 0.9.1 | All SLAAC addresses kept (one per prefix), link-local first; SLAAC's routes left to smoltcp; RDNSS servers as DNS servers; `Stack::ipv6_addresses`, `Stack::ipv6_router_flags`, `Stack::wait_ipv6_router_flags_change`, `Stack::set_dhcpv6_lease` | Needs the smoltcp patches above; no PR yet |

Building a copy on its own (tests, clippy) uses the other copies through
`software/patches/.cargo/config.toml`. The smoltcp copy has its own tests (`cargo test --lib` in
`software/patches/smoltcp`, about 670). Building the gateway shows a dozen
warnings from it: they are upstream's, for the feature set the gateway
uses, and don't fail `clippy -D warnings` (the copy is no workspace
member). smoltcp's `main` differs little from 0.13.1 in the changed files,
so the patches go upstream as they are.

## Updating

When a new release comes out: compare its changelog with the fixes above.
If it has all of them, delete the copy and its `[patch.crates-io]` line. If
not, copy the new release in (one commit), reapply the remaining fixes (one
commit each) and update the table.
