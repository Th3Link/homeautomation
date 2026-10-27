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

## Updating

When a new release comes out: compare its changelog with the fixes above.
If it has all of them, delete the copy and its `[patch.crates-io]` line. If
not, copy the new release in (one commit), reapply the remaining fixes (one
commit each) and update the table.
