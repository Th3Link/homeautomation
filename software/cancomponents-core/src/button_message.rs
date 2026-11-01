//! Wire format for the `ButtonEvent` and `PirSensor` CAN messages
//! (`CanMessageType::ButtonEvent`, `CanMessageType::PirSensor`), the one
//! the original C++ firmware sent and the gateway decodes.
//!
//! Byte layout (4 bytes, little-endian count):
//! `[button_num, state, count_lo, count_hi]`

use num_enum::{IntoPrimitive, TryFromPrimitive};

/// State of a physical button (or the presence sensor), as reported over
/// CAN. See [`crate::button_fsm`] for when each is sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
pub enum ButtonState {
    Released = 0,
    Pressed = 1,
    Hold = 2,
    Single = 3,
    Double = 4,
    Tripple = 5,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonMessage {
    pub num: u8,
    pub state: ButtonState,
    pub count: u16,
}

impl ButtonMessage {
    pub fn new(num: u8, state: ButtonState, count: u16) -> ButtonMessage {
        ButtonMessage { num, state, count }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, crate::DecodeError> {
        if data.len() < 4 {
            return Err(crate::DecodeError);
        }
        let state = ButtonState::try_from(data[1]).map_err(|_| crate::DecodeError)?;
        Ok(ButtonMessage {
            num: data[0],
            state,
            count: u16::from_le_bytes([data[2], data[3]]),
        })
    }

    pub fn to_bytes(&self) -> [u8; 4] {
        let count = self.count.to_le_bytes();
        [self.num, self.state as u8, count[0], count[1]]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_state_roundtrips_through_its_own_discriminant() {
        for state in [
            ButtonState::Released,
            ButtonState::Pressed,
            ButtonState::Hold,
            ButtonState::Single,
            ButtonState::Double,
            ButtonState::Tripple,
        ] {
            let byte: u8 = state.into();
            assert_eq!(ButtonState::try_from(byte), Ok(state));
        }
    }

    #[test]
    fn button_state_rejects_unknown_byte() {
        assert!(ButtonState::try_from(6u8).is_err());
        assert!(ButtonState::try_from(128u8).is_err());
    }

    #[test]
    fn count_is_little_endian() {
        let msg = ButtonMessage::new(5, ButtonState::Hold, 0x0102);
        assert_eq!(msg.to_bytes(), [5, 2, 0x02, 0x01]);
        assert_eq!(ButtonMessage::from_bytes(&msg.to_bytes()), Ok(msg));
    }

    #[test]
    fn from_bytes_rejects_short_input() {
        assert!(ButtonMessage::from_bytes(&[0, 1]).is_err());
    }
}
