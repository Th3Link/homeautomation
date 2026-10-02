//! Wire format for the `ButtonEvent`/`PirSensor` CAN messages
//! (`ICAN::BUTTON_EVENT_t` in the original C++). Both message types share
//! the same 4-byte payload shape; [`crate::topics::button`] renders them
//! into their (different) MQTT bodies.

use num_enum::{FromPrimitive, IntoPrimitive};

/// `ICAN::BUTTON_EVENT_t`. `Tripple` keeps the original's misspelling
/// deliberately — it's a wire-protocol discriminant name, not prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, IntoPrimitive, FromPrimitive)]
#[repr(u8)]
pub enum ButtonEventKind {
    #[num_enum(default)]
    Released = 0,
    Pressed = 1,
    Hold = 2,
    Single = 3,
    Double = 4,
    Tripple = 5,
}

/// A decoded `ButtonEvent`/`PirSensor` payload.
///
/// Wire layout (4 bytes): `[button_id, event, count(LE 16-bit)]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonEvent {
    pub button_id: u8,
    pub event: ButtonEventKind,
    pub count: u16,
}

impl ButtonEvent {
    pub fn to_bytes(self) -> [u8; 4] {
        let c = self.count.to_le_bytes();
        [self.button_id, self.event.into(), c[0], c[1]]
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, crate::DecodeError> {
        if data.len() < 4 {
            return Err(crate::DecodeError);
        }
        Ok(Self {
            button_id: data[0],
            event: ButtonEventKind::from(data[1]),
            count: u16::from_le_bytes([data[2], data[3]]),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let ev = ButtonEvent {
            button_id: 3,
            event: ButtonEventKind::Double,
            count: 1000,
        };
        assert_eq!(ButtonEvent::from_bytes(&ev.to_bytes()).unwrap(), ev);
    }

    #[test]
    fn rejects_short_input() {
        assert!(ButtonEvent::from_bytes(&[0u8; 3]).is_err());
    }

    #[test]
    fn unknown_event_byte_falls_back_to_released() {
        assert_eq!(ButtonEventKind::from(200), ButtonEventKind::Released);
    }

    #[test]
    fn all_named_discriminants_roundtrip() {
        for (kind, byte) in [
            (ButtonEventKind::Released, 0u8),
            (ButtonEventKind::Pressed, 1),
            (ButtonEventKind::Hold, 2),
            (ButtonEventKind::Single, 3),
            (ButtonEventKind::Double, 4),
            (ButtonEventKind::Tripple, 5),
        ] {
            assert_eq!(u8::from(kind), byte);
            assert_eq!(ButtonEventKind::from(byte), kind);
        }
    }
}
