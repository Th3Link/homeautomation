# Architecture Decision Records — gateway

Component-scoped ADRs for `gateway-core`/`gateway-hardware`. See the
[workspace-wide ADR index](../../../../docs/adr/index.md) for the full,
globally-numbered list including cross-cutting decisions.

| # | Title | Status |
|---|-------|--------|
| [0001](0001-workspace-split-core-hardware.md) | Split the port into `gateway-core` (logic) and `gateway-hardware` (peripherals) | Accepted |
| [0002](0002-ethernet-primary-wifi-fallback.md) | Ethernet is the primary network path, WiFi is a fallback, never both at once | Superseded by [0013](0013-ethernet-only-no-wifi.md) |
| [0005](0005-reimplement-wire-protocol-independently.md) | Reimplement the CAN/MQTT wire protocol independently rather than depend on `cancomponent-rs` | Superseded by [0012](../../../../docs/adr/0012-shared-common-crates.md) |
| [0006](0006-esp-radio-over-esp-wifi.md) | Use `esp-radio`, not `esp-wifi`, for WiFi | Superseded by [0013](0013-ethernet-only-no-wifi.md) |
| [0007](0007-mqtt5-client.md) | Use an MQTT 5.0 client (`rust-mqtt`) instead of matching the original's MQTT 3.1.1 | Accepted |
| [0008](0008-mandatory-web-authentication.md) | Require Basic Auth on every web endpoint unconditionally | Accepted |
| [0009](0009-promiscuous-can-dispatch.md) | Replace the dispatcher-list CAN fan-out with direct `msg_type` routing | Accepted |
| [0013](0013-ethernet-only-no-wifi.md) | Wired Ethernet is the only network path — drop WiFi | Accepted |
