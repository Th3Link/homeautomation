use crate::can_message_type::CanMessageType;
use embedded_can::ExtendedId;

/// A decoded 29-bit extended CAN ID used by the mqtt-gateway protocol.
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
    /// recognize "the same device, some message" — see
    /// [`crate::device_list`].
    pub fn device_key(self) -> u32 {
        u32::from(self) & 0xFFFF_FF00
    }
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
}
