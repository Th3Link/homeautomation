//! On-gateway automation: building blocks that turn CAN events into CAN
//! commands on the gateway itself, without MQTT or a hub (ADR 0025).
//!
//! - [`output`]: what can be switched, named once: room lights in levels,
//!   dimmers, relays, roller shutters and groups of them.
//! - [`binding`]: a button event and what it does.
//! - [`motion`]: a light switched by motion detectors, with a timeout, a
//!   lockout after switching off and a manual mode.
//! - [`schedule`]: an output switched in time windows, by the clock, the sun
//!   or the brightness.
//! - [`when`]: the times and conditions the last two use.
//! - [`engine`]: the state of all of them and what to send when something
//!   happens.
//!
//! Everything here is pure: callers pass in the time and the inputs and
//! get back the commands to send. The configurations are `serde` types:
//! JSON on the REST API, `postcard` on flash ([`encode`], [`decode`]).

pub mod binding;
pub mod engine;
pub mod motion;
pub mod output;
pub mod schedule;
pub mod store;
pub mod when;

use crate::can_id::CanId;
use crate::can_message_type::CanMessageType;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// The longest name of an output, motion light or schedule.
pub const NAME_CAP: usize = 20;

/// How many of each there can be.
pub const MAX_OUTPUTS: usize = 48;
pub const MAX_BINDINGS: usize = 128;
pub const MAX_MOTION_LIGHTS: usize = 12;
pub const MAX_SCHEDULES: usize = 8;

/// A name: letters, digits, `-` and `_`, 1 to [`NAME_CAP`] bytes.
pub type Name = heapless::String<NAME_CAP, u8>;

/// Whether `name` is a valid [`Name`].
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= NAME_CAP
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

/// A CAN device by its address: device type and id (1–255).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceRef {
    #[serde(rename = "type")]
    pub device_type: u8,
    pub id: u8,
}

impl DeviceRef {
    /// The device's CAN id without the message type.
    pub fn key(self) -> u32 {
        CanId::new(self.device_type, self.id, CanMessageType::InvalidMessage).device_key()
    }

    pub fn from_key(key: u32) -> Self {
        let id = CanId::from(key);
        Self {
            device_type: id.device_type,
            id: id.device_id,
        }
    }

    fn valid(self) -> bool {
        self.device_type < 64 && self.id != 0
    }
}

/// Why a configuration was refused: the field and what is wrong with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Invalid {
    pub field: &'static str,
    pub reason: &'static str,
}

pub(crate) fn invalid<T>(field: &'static str, reason: &'static str) -> Result<T, Invalid> {
    Err(Invalid { field, reason })
}

/// The format version byte in front of every stored configuration.
pub const FORMAT_VERSION: u8 = 1;

/// Writes `value` to `buffer` as postcard behind [`FORMAT_VERSION`], and
/// returns the bytes used. `None` if it doesn't fit.
pub fn encode<'b, T: Serialize>(value: &T, buffer: &'b mut [u8]) -> Option<&'b [u8]> {
    let (version, rest) = buffer.split_first_mut()?;
    *version = FORMAT_VERSION;
    let len = postcard::to_slice(value, rest).ok()?.len();
    Some(&buffer[..=len])
}

/// Reads what [`encode`] wrote. `None` for another format version or bytes
/// that don't decode.
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Option<T> {
    let (&version, rest) = bytes.split_first()?;
    if version != FORMAT_VERSION {
        return None;
    }
    postcard::from_bytes(rest).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert!(valid_name("bad-pir_1"));
        assert!(!valid_name(""));
        assert!(!valid_name("bad pir"));
        assert!(!valid_name("küche"));
        assert!(!valid_name(&"a".repeat(NAME_CAP + 1)));
    }

    #[test]
    fn device_keys() {
        let lamps = DeviceRef {
            device_type: 3,
            id: 6,
        };
        assert_eq!(lamps.key(), 0x1003_0600);
        assert_eq!(DeviceRef::from_key(0x1003_0600), lamps);
    }

    #[test]
    fn stored_form_round_trips_and_checks_the_version() {
        let device = DeviceRef {
            device_type: 5,
            id: 0x80,
        };
        let mut buffer = [0u8; 16];
        let bytes = encode(&device, &mut buffer).unwrap().to_vec();
        assert_eq!(bytes[0], FORMAT_VERSION);
        assert_eq!(decode::<DeviceRef>(&bytes), Some(device));
        let mut other = bytes.clone();
        other[0] = FORMAT_VERSION + 1;
        assert_eq!(decode::<DeviceRef>(&other), None);
        assert!(encode(&device, &mut [0u8; 2]).is_none());
    }
}
