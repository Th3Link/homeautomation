# Architecture Decision Records

Each ADR follows the same template: **Status**, **Context**, **Decision**,
**Consequences**. Once accepted, an ADR's decision text is not edited —
if circumstances change, a new ADR supersedes it and says so explicitly.

| # | Title | Status |
|---|-------|--------|
| [0001](0001-workspace-split-core-hardware.md) | Split the port into `gateway-core` (logic) and `gateway-hardware` (peripherals) | Accepted |
| [0002](0002-ethernet-primary-wifi-fallback.md) | Ethernet is the primary network path, WiFi is a fallback, never both at once | Accepted |
| [0003](0003-mqtt-transport-to-home-automation-hub.md) | MQTT as the transport to openHAB/Home Assistant | Accepted |
| [0004](0004-can-bus-inter-component-communication.md) | CAN bus for inter-component communication | Accepted |
| [0005](0005-reimplement-wire-protocol-independently.md) | Reimplement the CAN/MQTT wire protocol independently rather than depend on `cancomponent-rs` | Accepted |
| [0006](0006-esp-radio-over-esp-wifi.md) | Use `esp-radio`, not `esp-wifi`, for WiFi | Accepted |
| [0007](0007-mqtt5-client.md) | Use an MQTT 5.0 client (`rust-mqtt`) instead of matching the original's MQTT 3.1.1 | Accepted |
| [0008](0008-mandatory-web-authentication.md) | Require Basic Auth on every web endpoint unconditionally | Accepted |
| [0009](0009-promiscuous-can-dispatch.md) | Replace the dispatcher-list CAN fan-out with direct `msg_type` routing | Accepted |
| [0010](0010-can-ota-rework-planned.md) | Rework CAN-bus OTA into a chunked, acknowledged, retransmitting protocol | Proposed |
| [0011](0011-gateway-mesh-planned.md) | Multiple gateways, mesh-aware, with on-gateway automation independent of MQTT | Proposed |

ADRs 0010 and 0011 are **proposed**, not yet implemented — see
[`../technical-debt.md`](../technical-debt.md) for the problems driving them
and [`../requirements.md`](../requirements.md) for how they fit the
project's planned direction.
