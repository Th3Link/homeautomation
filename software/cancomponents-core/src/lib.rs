#![no_std]
//! Hardware-independent CAN wire format and pure control logic shared by
//! the cancomponents firmware. Everything here is `#![no_std]` and, aside
//! from what needs `embassy-time`'s `Instant`/`Duration` types (which don't
//! require a running time driver unless you call `Instant::now()`), fully
//! host-testable with `cargo test`.
/// A wire-format decode failed: the input was too short or contained an
/// invalid discriminant. Carries no further detail — callers only need to
/// know decoding didn't succeed, matching how these bytes arrive off a CAN
/// bus with no way to ask the sender to retry.
///
/// Re-exported from [`common_core`], shared with `gateway-core`.
pub use common_core::DecodeError;

/// The CAN wire-format types shared with the gateway (`gateway-core`) —
/// re-exported from [`common_core`] so existing `crate::can_id`/
/// `crate::can_message_type`/`crate::device_type`/`crate::device_message`
/// paths keep working unchanged.
pub use common_core::{can_id, can_message_type, device_message, device_type};

pub mod button_fsm;
pub mod button_message;
pub mod docs;
pub mod error;
pub mod extension;
pub mod relais;
pub mod relais_manager;
