#![no_std]
//! Hardware-independent wire format and translation logic for the
//! mqtt-gateway firmware.
//!
//! This crate is the Rust rewrite of the parts of the original C++/ESP-IDF
//! `mqtt_gateway` that don't touch hardware: the CAN wire format spoken to
//! bus nodes, the CAN\<-\>MQTT topic translation done by the gateway's
//! `Bridge*` classes, the device registry, the `/control.json` RPC model,
//! the CAN-bus OTA orchestration, and the consolidated configuration
//! schema. Everything here is `#![no_std]`, deterministic (callers pass in
//! `now`/state explicitly rather than this crate reading a clock or doing
//! I/O), and fully host-testable with `cargo test` — no hardware needed.
//!
//! Actual peripheral access (CAN/TWAI, Ethernet, MQTT sockets, flash storage,
//! the HTTP config server) lives in the sibling `gateway-hardware` crate,
//! which drives the types and pure functions defined here.

/// A wire-format decode failed: the input was too short, contained an
/// invalid discriminant, or otherwise didn't parse. Carries no further
/// detail — callers only need to know decoding didn't succeed, matching how
/// these bytes arrive off a CAN bus or an MQTT payload with no way to ask
/// the sender to retry.
///
/// Re-exported from [`common_core`], shared with `cancomponents-core`.
pub use common_core::DecodeError;

/// The CAN wire-format types shared with the CAN node firmware
/// (`cancomponents-core`) — re-exported from [`common_core`] so existing
/// `crate::can_id`/`crate::can_message_type`/`crate::device_type`/
/// `crate::device_message` paths keep working unchanged.
pub use common_core::{can_id, can_message_type, device_message, device_type};

pub mod button;
pub mod can_ota;
pub mod command;
pub mod config;
pub mod device_list;
pub mod docs;
pub mod error;
pub mod helper;
pub mod lamp;
pub mod relais;
pub mod sensor;
pub mod topics;
