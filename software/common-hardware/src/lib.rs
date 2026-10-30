#![no_std]
//! Shared ESP32 hardware-support helpers used by both firmware crates
//! (`gateway-hardware` and `cancomponents-hardware`): exclusive on-chip
//! flash access ([`flash`]), a console-aware debug logging macro
//! ([`logging`]), the `sequential-storage`-backed key/value config
//! store boilerplate ([`config_store`]) and the reset-instead-of-halt
//! panic hook ([`reset`]). Anything that needs a crate-local
//! type — the actual config schema/`Key` enum, the CLI itself — stays in
//! each firmware crate; only the genuinely identical plumbing lives here.

pub mod config_store;
pub mod flash;
pub mod logging;
pub mod reset;
