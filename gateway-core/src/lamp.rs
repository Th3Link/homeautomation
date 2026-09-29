//! Wire format for the `LampGroup` and `Nightlight` CAN messages
//! (`ICAN::LAMP_MSG_t` in the original C++).
//!
//! Like [`crate::relais`], `LampGroup` has two different wire shapes in
//! practice, both reproduced here:
//! - MQTT (`BridgeLamps::dispatch`, `canbus/lamp_command/#`): 8 bytes with
//!   `bank` when the command body supplies one, otherwise 4 bytes
//!   (`value`+`bitmask` only) — [`LampMsg::to_bytes_full`] /
//!   [`LampMsg::to_bytes_short`].
//! - The `/control.json` RPC (`Command::lamps`): **always** 4 bytes, even
//!   though a `bank` field is accepted in the JSON body — it's parsed but
//!   never actually placed on the wire. Reproduced as-is: callers using
//!   the RPC-style encode should use [`LampMsg::to_bytes_short`]
//!   regardless of whether `bank` is meaningful, matching the original's
//!   quirk rather than silently "fixing" it into a different wire message
//!   than the real firmware sends.

/// A `LampGroup` command/state. Wire layout: `[value, bitmask(LE 24-bit),
/// bank, reserved(24-bit)]` when sent in full (8 bytes), or just `[value,
/// bitmask(LE 24-bit)]` (4 bytes) when `bank` is omitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LampMsg {
    pub value: u8,
    pub bitmask: u32,
    pub bank: u8,
}

impl LampMsg {
    pub fn to_bytes_full(self) -> [u8; 8] {
        let b = (self.bitmask & 0x00FF_FFFF).to_le_bytes();
        [self.value, b[0], b[1], b[2], self.bank, 0, 0, 0]
    }

    pub fn to_bytes_short(self) -> [u8; 4] {
        let b = (self.bitmask & 0x00FF_FFFF).to_le_bytes();
        [self.value, b[0], b[1], b[2]]
    }

    /// Decodes either the 4-byte (`bank` defaults to `0`) or 8-byte form.
    pub fn from_bytes(data: &[u8]) -> Result<Self, crate::DecodeError> {
        if data.len() < 4 {
            return Err(crate::DecodeError);
        }
        Ok(Self {
            value: data[0],
            bitmask: u32::from_le_bytes([data[1], data[2], data[3], 0]),
            bank: data.get(4).copied().unwrap_or(0),
        })
    }
}

/// Encodes a `Nightlight` command: a single raw byte, no framing.
pub fn nightlight_bytes(value: u8) -> [u8; 1] {
    [value]
}

/// Decodes a `Nightlight` payload.
pub fn decode_nightlight(data: &[u8]) -> Option<u8> {
    data.first().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_roundtrip() {
        let msg = LampMsg {
            value: 0xAB,
            bitmask: 0x00FF_00FF,
            bank: 2,
        };
        assert_eq!(LampMsg::from_bytes(&msg.to_bytes_full()).unwrap(), msg);
    }

    #[test]
    fn short_form_defaults_bank_to_zero() {
        let msg = LampMsg {
            value: 0xAB,
            bitmask: 0x0000_00FF,
            bank: 0,
        };
        let bytes = msg.to_bytes_short();
        assert_eq!(bytes.len(), 4);
        assert_eq!(LampMsg::from_bytes(&bytes).unwrap(), msg);
    }

    #[test]
    fn short_form_drops_bank_even_if_nonzero() {
        // Matches the /control.json RPC quirk: bank is accepted but never
        // actually transmitted on the 4-byte wire form.
        let msg = LampMsg {
            value: 1,
            bitmask: 2,
            bank: 99,
        };
        let bytes = msg.to_bytes_short();
        let decoded = LampMsg::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.bank, 0);
    }

    #[test]
    fn bitmask_truncates_to_24_bits() {
        let msg = LampMsg {
            value: 0,
            bitmask: (1 << 24) - 1,
            bank: 0,
        };
        assert_eq!(
            LampMsg::from_bytes(&msg.to_bytes_full()).unwrap().bitmask,
            (1 << 24) - 1
        );
    }

    #[test]
    fn rejects_short_input() {
        assert!(LampMsg::from_bytes(&[0u8; 3]).is_err());
    }

    #[test]
    fn nightlight_roundtrip() {
        assert_eq!(decode_nightlight(&nightlight_bytes(42)), Some(42));
        assert_eq!(decode_nightlight(&[]), None);
    }
}
