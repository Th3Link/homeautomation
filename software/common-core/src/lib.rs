#![no_std]
//! CAN wire-format types shared by every component on the bus:
//! `gateway-core` (the CAN↔MQTT bridge) and `cancomponents-core` (the node
//! firmware — relays, lights, buttons). Only the parts that are genuinely
//! identical on both sides live here — [`can_id`], [`can_message_type`],
//! [`device_type`], and [`device_message`]'s `DeviceIdType` codec. Anything
//! that differs between the gateway and node roles (error reporting,
//! relay-command shapes, ...) stays in each crate's own `error`/`relais`
//! modules rather than being forced into a shared shape here.
//!
//! `#![no_std]`, no hardware I/O, fully host-testable with `cargo test`.

/// A wire-format decode failed: the input was too short, contained an
/// invalid discriminant, or otherwise didn't parse. Carries no further
/// detail — callers only need to know decoding didn't succeed, matching how
/// these bytes arrive off a CAN bus with no way to ask the sender to retry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeError;

impl core::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("malformed CAN message data")
    }
}

impl core::error::Error for DecodeError {}

pub mod can_id;
pub mod can_message_type;
pub mod device_message;
pub mod device_type;
