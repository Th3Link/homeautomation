//! Project documentation, pulled into `cargo doc` output via
//! `#[doc = include_str!(...)]` — see `gateway_core::docs` for the fuller
//! version of this pattern. Nothing in this module has any code.

/// Workspace-wide requirements — what the whole system does, and why.
#[doc = include_str!("../../../docs/requirements.md")]
pub mod requirements {}

/// The workspace-wide ADR index (all scopes).
#[doc = include_str!("../../../docs/adr/index.md")]
pub mod adr_index {}

/// The CAN wire protocol specification (workspace-wide — shared with
/// `gateway-core`).
#[doc = include_str!("../../../docs/protocol-can.md")]
pub mod protocol_can {}

/// Component-specific docs placeholder — see the module's own text.
#[doc = include_str!("../docs/index.md")]
pub mod component_docs {}
