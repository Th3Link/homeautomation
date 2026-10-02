use crate::can_message_type::CanMessageType;
use embedded_can::ExtendedId;

/// A decoded 29-bit extended CAN ID, shared by every device on the bus —
/// the gateway and the CAN node firmware alike.
///
/// Bit layout, MSB first (29 bits total, matching an extended CAN ID):
/// `is_ng`(1) | `group`(6) | `device_type`(6) | `device_id`(8) |
/// `msg_type`(8). Matches the original C++ `ICAN::ID_*_MASK` constants
/// bit-for-bit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanId {
    /// "Next generation" protocol marker; always `true` for anything this
    /// crate sends. 1 bit.
    pub is_ng: bool,
    /// Reserved for grouping/broadcast domains; always `0` in practice. 6 bit.
    pub group: u8,
    /// Device type, see [`crate::device_type::DeviceType`]. 6 bit.
    pub device_type: u8,
    /// Target/source device id; `0` is the broadcast address. 8 bit.
    pub device_id: u8,
    /// Message type, see [`CanMessageType`]. 8 bit.
    pub msg_type: CanMessageType,
}

impl CanId {
    pub fn new(device_type: u8, device_id: u8, msg_type: CanMessageType) -> Self {
        Self {
            is_ng: true,
            group: 0,
            device_type: device_type & 0x3F, // 6 bit
            device_id,
            msg_type,
        }
    }

    /// The frame's identity without its `msg_type` — `is_ng|group|
    /// device_type|device_id` only, with `msg_type` zeroed. This is the key
    /// the original C++ groups CAN frames by (`identifier & 0xFFFFFF00`) to
    /// recognize "the same device, some message".
    pub fn device_key(self) -> u32 {
        u32::from(self) & 0xFFFF_FF00
    }
}

/// Computes the ESP32 TWAI dual-extended-filter `(code, mask)` register
/// pairs that accept frames addressed to `device_type`/`device_id` (plus
/// broadcast frames, via the second code/mask pair matching any "is_ng"
/// frame). Only the top 3 bits of `device_id` actually participate in the
/// hardware match (the filter only compares the upper 16 bits of the ID
/// register pair), so this is a coarse pre-filter — callers still re-check
/// the full `device_id` themselves after receiving a frame. Used by CAN
/// node firmware that wants hardware-filtered receive; the gateway itself
/// dispatches promiscuously instead (see its own ADRs) and doesn't call
/// this.
///
/// Returns `(code1, mask1, code2, mask2)` for
/// `DualExtendedFilter::new_from_code_mask([code1, code2], [mask1, mask2])`.
///
/// This is pure bit arithmetic with no hardware dependency, but the exact
/// formula reflects the ESP32 TWAI peripheral's specific filter register
/// layout — treat the numbers this produces as a fixed contract, not
/// something to "simplify".
pub fn filter_code_mask(device_type: u8, device_id: u8) -> (u16, u16, u16, u16) {
    const IS_NG_BIT: u32 = 1 << 28;
    const FULL_MASK: u32 = 0x103FFF00;

    let full_id =
        IS_NG_BIT | ((device_type as u32 & 0x3F) << 16) | ((device_id as u32 & 0xFF) << 8);
    let code1 = ((full_id >> 13) & 0xFFFF) as u16;
    let mask1 = ((FULL_MASK >> 13) & 0xFFFF) as u16;

    // Second code/mask pair: accept any "is_ng" frame regardless of type/id,
    // so broadcast (device_id == 0) frames still reach the dispatcher.
    let code2 = ((IS_NG_BIT >> 13) & 0xFFFF) as u16;

    (code1, mask1, code2, mask1)
}

impl From<CanId> for u32 {
    fn from(id: CanId) -> Self {
        ((id.is_ng as u32) << 28)
            | ((id.group as u32 & 0x3F) << 22)
            | ((id.device_type as u32 & 0x3F) << 16)
            | ((id.device_id as u32) << 8)
            | (u8::from(id.msg_type) as u32)
    }
}

impl From<CanId> for ExtendedId {
    fn from(id: CanId) -> Self {
        ExtendedId::new(id.into()).expect("CanId always fits in 29 bits")
    }
}

impl From<u32> for CanId {
    fn from(raw: u32) -> Self {
        Self {
            is_ng: ((raw >> 28) & 0x1) != 0,
            group: ((raw >> 22) & 0x3F) as u8,
            device_type: ((raw >> 16) & 0x3F) as u8,
            device_id: ((raw >> 8) & 0xFF) as u8,
            msg_type: CanMessageType::from((raw & 0xFF) as u8),
        }
    }
}

impl From<ExtendedId> for CanId {
    fn from(id: ExtendedId) -> Self {
        Self::from(id.as_raw())
    }
}

impl core::fmt::Display for CanId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "NG:{} Group:{} Type:{} ID:{} Msg:{:?}",
            self.is_ng, self.group, self.device_type, self.device_id, self.msg_type
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_through_u32() {
        let id = CanId::new(0x05, 0x2A, CanMessageType::RelaisState);
        let raw: u32 = id.into();
        assert_eq!(CanId::from(raw), id);
    }

    #[test]
    fn roundtrips_through_extended_id() {
        let id = CanId::new(0x06, 0x01, CanMessageType::Available);
        let ext: ExtendedId = id.into();
        assert_eq!(CanId::from(ext), id);
    }

    #[test]
    fn new_masks_device_type_to_6_bits() {
        let id = CanId::new(0xFF, 0x01, CanMessageType::Available);
        assert_eq!(id.device_type, 0x3F);
    }

    #[test]
    fn device_key_zeroes_msg_type_only() {
        let id = CanId::new(0x05, 0x2A, CanMessageType::RelaisState);
        let key = id.device_key();
        assert_eq!(key & 0xFF, 0);
        assert_eq!(key, u32::from(id) & 0xFFFF_FF00);
    }

    #[test]
    fn matches_hand_derived_bit_layout() {
        // is_ng=1, group=0, device_type=0x05, device_id=0x2A, msg_type=130 (Relais)
        let id = CanId::new(0x05, 0x2A, CanMessageType::Relais);
        let raw: u32 = id.into();
        assert_eq!(raw, 0x1005_2A82);
    }

    /// Regression test pinning the filter register values for a known
    /// input, hand-derived from the formula in `filter_code_mask`. Guards
    /// the shift/mask arithmetic against accidental changes during
    /// refactors — it is not a claim that these values are "correct" in
    /// some independently-verifiable sense (that would need the real TWAI
    /// peripheral).
    #[test]
    fn filter_code_mask_matches_hand_derived_values() {
        let (code1, mask1, code2, mask2) = filter_code_mask(0x01, 0x01);
        assert_eq!(code1, 0x8008);
        assert_eq!(mask1, 0x81FF);
        assert_eq!(code2, 0x8000);
        assert_eq!(mask2, 0x81FF);
    }

    #[test]
    fn filter_code_mask_second_pair_is_device_independent() {
        // The second (code, mask) pair only asserts the "is_ng" bit, so it
        // must be identical regardless of device_type/device_id.
        let (_, _, code2_a, mask2_a) = filter_code_mask(0x01, 0x01);
        let (_, _, code2_b, mask2_b) = filter_code_mask(0x3F, 0xFF);
        assert_eq!(code2_a, code2_b);
        assert_eq!(mask2_a, mask2_b);
    }
}
