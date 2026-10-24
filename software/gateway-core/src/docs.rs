//! Project documentation — requirements, architecture decisions, the CAN
//! and MQTT wire protocol specifications, and known technical debt.
//!
//! This is the same content as the plain markdown files under the
//! repository-root `docs/` (cross-cutting, workspace-wide) and this
//! crate's own `docs/` (gateway-specific), readable directly on GitHub or
//! with any editor, pulled in here via `#[doc = include_str!(...)]` so it
//! also renders as part of this crate's `cargo doc` output and gets
//! published alongside the API reference. Nothing in this module has any
//! code — every item below is an empty marker `mod` that exists purely to
//! carry a doc page.

/// Workspace-wide requirements — what the whole system does, and why.
#[doc = include_str!("../../../docs/requirements.md")]
pub mod requirements {}

/// The gateway's own requirements (legacy parity, the `/control.json`
/// API, planned changes).
#[doc = include_str!("../docs/requirements.md")]
pub mod gateway_requirements {}

/// Architecture Decision Records.
pub mod adr {
    /// The workspace-wide ADR index (all scopes).
    #[doc = include_str!("../../../docs/adr/index.md")]
    pub mod index {}

    /// The gateway-scoped ADR index.
    #[doc = include_str!("../docs/adr/index.md")]
    pub mod gateway_index {}

    #[doc = include_str!("../docs/adr/0001-workspace-split-core-hardware.md")]
    pub mod adr_0001_workspace_split_core_hardware {}

    #[doc = include_str!("../docs/adr/0002-ethernet-primary-wifi-fallback.md")]
    pub mod adr_0002_ethernet_primary_wifi_fallback {}

    #[doc = include_str!("../../../docs/adr/0003-mqtt-transport-to-home-automation-hub.md")]
    pub mod adr_0003_mqtt_transport_to_home_automation_hub {}

    #[doc = include_str!("../../../docs/adr/0004-can-bus-inter-component-communication.md")]
    pub mod adr_0004_can_bus_inter_component_communication {}

    #[doc = include_str!("../docs/adr/0005-reimplement-wire-protocol-independently.md")]
    pub mod adr_0005_reimplement_wire_protocol_independently {}

    #[doc = include_str!("../docs/adr/0006-esp-radio-over-esp-wifi.md")]
    pub mod adr_0006_esp_radio_over_esp_wifi {}

    #[doc = include_str!("../docs/adr/0007-mqtt5-client.md")]
    pub mod adr_0007_mqtt5_client {}

    #[doc = include_str!("../docs/adr/0008-mandatory-web-authentication.md")]
    pub mod adr_0008_mandatory_web_authentication {}

    #[doc = include_str!("../docs/adr/0009-promiscuous-can-dispatch.md")]
    pub mod adr_0009_promiscuous_can_dispatch {}

    #[doc = include_str!("../../../docs/adr/0010-can-ota-rework-planned.md")]
    pub mod adr_0010_can_ota_rework_planned {}

    #[doc = include_str!("../../../docs/adr/0011-gateway-mesh-planned.md")]
    pub mod adr_0011_gateway_mesh_planned {}

    #[doc = include_str!("../../../docs/adr/0012-shared-common-crates.md")]
    pub mod adr_0012_shared_common_crates {}

    #[doc = include_str!("../docs/adr/0013-ethernet-only-no-wifi.md")]
    pub mod adr_0013_ethernet_only_no_wifi {}
}

/// The CAN wire protocol specification (workspace-wide — shared with
/// `cancomponents-core`).
#[doc = include_str!("../../../docs/protocol-can.md")]
pub mod protocol_can {}

/// The MQTT topic scheme and payload format specification (gateway-only).
#[doc = include_str!("../docs/protocol-mqtt.md")]
pub mod protocol_mqtt {}

/// Known technical debt and unfinished work (gateway-only).
#[doc = include_str!("../docs/technical-debt.md")]
pub mod technical_debt {}
