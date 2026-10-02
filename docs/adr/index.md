# Architecture Decision Records

Each ADR follows the same template: **Status**, **Context**, **Decision**,
**Consequences**. Once accepted, an ADR's decision text is not edited — if
circumstances change, a new ADR supersedes it and says so explicitly.

ADRs are numbered in one global sequence regardless of where they live.
**Workspace**-scoped ADRs (decisions that constrain more than one
component) live here, at the repository root. Component-scoped ADRs live
next to the component they constrain — e.g.
[`software/gateway-core/docs/adr/`](../../software/gateway-core/docs/adr/index.md)
— and are cross-referenced below.

| # | Title | Scope | Status |
|---|-------|-------|--------|
| [0001](../../software/gateway-core/docs/adr/0001-workspace-split-core-hardware.md) | Split the port into `gateway-core` (logic) and `gateway-hardware` (peripherals) | gateway | Accepted |
| [0002](../../software/gateway-core/docs/adr/0002-ethernet-primary-wifi-fallback.md) | Ethernet is the primary network path, WiFi is a fallback, never both at once | gateway | Accepted |
| [0003](0003-mqtt-transport-to-home-automation-hub.md) | MQTT as the transport to openHAB/Home Assistant | workspace | Accepted |
| [0004](0004-can-bus-inter-component-communication.md) | CAN bus for inter-component communication | workspace | Accepted |
| [0005](../../software/gateway-core/docs/adr/0005-reimplement-wire-protocol-independently.md) | Reimplement the CAN/MQTT wire protocol independently rather than depend on `cancomponent-rs` | gateway | Superseded by 0012 |
| [0006](../../software/gateway-core/docs/adr/0006-esp-radio-over-esp-wifi.md) | Use `esp-radio`, not `esp-wifi`, for WiFi | gateway | Accepted |
| [0007](../../software/gateway-core/docs/adr/0007-mqtt5-client.md) | Use an MQTT 5.0 client (`rust-mqtt`) instead of matching the original's MQTT 3.1.1 | gateway | Accepted |
| [0008](../../software/gateway-core/docs/adr/0008-mandatory-web-authentication.md) | Require Basic Auth on every web endpoint unconditionally | gateway | Accepted |
| [0009](../../software/gateway-core/docs/adr/0009-promiscuous-can-dispatch.md) | Replace the dispatcher-list CAN fan-out with direct `msg_type` routing | gateway | Accepted |
| [0010](0010-can-ota-rework-planned.md) | Rework CAN-bus OTA into a chunked, acknowledged, retransmitting protocol | workspace | Proposed |
| [0011](0011-gateway-mesh-planned.md) | Multiple gateways, mesh-aware, with on-gateway automation independent of MQTT | workspace | Proposed |
| [0012](0012-shared-common-crates.md) | Extract `common-core`/`common-hardware`, superseding independent reimplementation | workspace | Accepted |

ADRs 0010 and 0011 are **proposed**, not yet implemented — see
[`gateway-core`'s technical-debt.md`](../../software/gateway-core/docs/technical-debt.md)
for the problems driving them.

`cancomponents-core`/`cancomponents-hardware` don't have component-scoped
ADRs yet — see
[`software/cancomponents-core/docs/`](../../software/cancomponents-core/docs/index.md).
