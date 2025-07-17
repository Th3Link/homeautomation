use crate::can_message_type::CanMessageType;
use embedded_can::ExtendedId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanId {
    pub is_ng: bool,              // 1 Bit
    pub group: u8,                // 6 Bit
    pub device_type: u8,          // 6 Bit
    pub device_id: u8,            // 8 Bit
    pub msg_type: CanMessageType, // 8 Bit
}

impl CanId {
    pub fn new(device_type: u8, device_id: u8, msg_type: CanMessageType) -> Self {
        Self {
            is_ng: true,
            group: 0,
            device_type: device_type & 0x3F, // nur 6 Bit
            device_id,
            msg_type,
        }
    }
}

impl From<CanId> for u32 {
    fn from(id: CanId) -> Self {
        ((id.is_ng as u32) << 28)
            | ((id.group as u32 & 0x3F) << 22)
            | ((id.device_type as u32 & 0x3F) << 16)
            | ((id.device_id as u32) << 8)
            | id.msg_type as u32 & 0xFF
    }
}

impl From<CanId> for ExtendedId {
    fn from(id: CanId) -> Self {
        ExtendedId::new(id.into()).expect("can id cannot be converted")
    }
}

impl From<u32> for CanId {
    fn from(raw: u32) -> Self {
        let msg_type = CanMessageType::from((raw & 0xFF) as u8);
        Self {
            is_ng: ((raw >> 28) & 0x1) != 0,
            group: ((raw >> 22) & 0x3F) as u8,
            device_type: ((raw >> 16) & 0x3F) as u8,
            device_id: ((raw >> 8) & 0xFF) as u8,
            msg_type,
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
    fn test_device_init() {
        let can_id = CanId::new(0x01, 0x01, CanMessageType::Nightlight);
        let can_id_u32: u32 = can_id.into();
        let can_id_back = CanId::try_from(can_id_u32);
        assert!(can_id_back.is_ok());

        let can_id_ext = TryInto::<ExtendedId>::try_into(can_id);
        assert!(can_id_ext.is_ok());

        let can_id_ext_back = TryInto::<CanId>::try_into(can_id_ext.unwrap());
        assert!(can_id_ext_back.is_ok())
    }
}
