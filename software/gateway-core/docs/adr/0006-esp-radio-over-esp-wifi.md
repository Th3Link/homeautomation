# ADR 0006: Use `esp-radio`, not `esp-wifi`, for WiFi

**Status:** Superseded by [ADR 0013](0013-ethernet-only-no-wifi.md)

## Context

`esp-hal` 1.2.2 (the version this project targets, matching
`cancomponent-rs`) pins `xtensa-lx-rt ^0.23.0`. The `esp-wifi` crate
(`0.15.1`, the only version published to crates.io at the time of the
port) pins `xtensa-lx-rt ^0.20.0`. Both crates declare a native library
`links` key on `xtensa-lx-rt`, and Cargo only permits one version of a
`links`-declared crate in the whole dependency graph — so `esp-wifi` and
`esp-hal` 1.2.2 simply cannot appear together in one build; this isn't a
version range that a `Cargo.toml` tweak can resolve.

`esp-wifi` is deprecated: its WiFi/Bluetooth/ESP-NOW functionality has
moved into a new crate, `esp-radio`, which is developed directly alongside
`esp-hal` in the same repository/release cadence. `esp-radio 1.0.0-beta.1`
declares a compatible `esp-hal` requirement and resolves cleanly against
1.2.2 with no conflict.

## Decision

Use `esp-radio` (currently pinned at `1.0.0-beta.1`) for WiFi station/
access-point support, not `esp-wifi`.

## Consequences

- `esp-radio` is pre-1.0 (`1.0.0-beta.1`); its API may still change before
  a stable 1.0 release. Bumping it later may require adjusting
  `gateway-hardware/src/wifi.rs`.
- `esp-radio`'s WiFi feature needs a real heap (`esp-alloc`) for its
  internal buffers — the only place in `gateway-hardware` that isn't
  pure `heapless`/static allocation. See `esp_alloc::heap_allocator!` in
  `main.rs`.
- This is a case where trusting direct, current knowledge of the esp-rs
  ecosystem's actual direction mattered more than what a crates.io/
  dependency-graph search alone would suggest — a naive search finds
  `esp-wifi` as "the" WiFi crate for esp-hal, since it's still the only
  thing published under that specific name, without surfacing that it's
  been superseded.
