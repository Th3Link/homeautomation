//! Sensor management of a node ([`CanMessageType::SensorConfig`]): the list of
//! its temperature sensors, their names and their roles in the heating
//! controller.
//!
//! A sensor is called by its **slot**: its number in the order the node found
//! the sensors since it started (0-31). A slot is local to the node and
//! temporary — after a restart, or when sensors come and go, it may mean
//! another sensor — so a command that matters carries a *guard*, the first
//! four bytes of the serial the sender believes the slot to have, and the
//! node refuses it if that is not so. Names stay with the sensor (the node
//! keeps them by serial), slots don't.
//!
//! Byte 0 of every frame: bits 0-4 slot, bits 5-7 operation. The other bytes
//! depend on the operation:
//!
//! | op | frame | bytes 1..8 |
//! |---|---|---|
//! | 0 name | set (to the node) or reported (from it) | the name, 7 bytes, NUL padded |
//! | 1 info | request for one slot (1 byte long) / the answer | serial (6), flags: bit 0 lost, bit 1 settling, bits 4-7 role |
//! | 2 role | assign a role | role (1), guard (4 bytes) |
//! | 3 forget | forget the sensors that are lost | – |
//!
//! An R frame of this message type asks for the whole list: the node answers
//! with an info and a name frame for every sensor it knows.
//!
//! [`CanMessageType::SensorConfig`]: crate::can_message_type::CanMessageType::SensorConfig

use crate::{DecodeError, EncodeError, Payload};

/// Most sensors a node handles (slots 0-31).
pub const MAX_SLOTS: u8 = 32;
/// Length of a sensor name.
pub const NAME_LEN: usize = 7;

/// A sensor's name: 1-7 characters out of `A-Z a-z 0-9 - _`, so that it is
/// safe in an MQTT topic and on a command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SensorName {
    bytes: [u8; NAME_LEN],
}

impl SensorName {
    /// `None` for an empty or too long name, or one with another character.
    pub fn new(text: &str) -> Option<Self> {
        let text = text.as_bytes();
        if text.is_empty()
            || text.len() > NAME_LEN
            || !text
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || *b == b'-' || *b == b'_')
        {
            return None;
        }
        let mut bytes = [0u8; NAME_LEN];
        bytes[..text.len()].copy_from_slice(text);
        Some(Self { bytes })
    }

    /// The name from its wire form; `None` if it isn't a valid one.
    pub fn from_wire(bytes: &[u8; NAME_LEN]) -> Option<Self> {
        let len = bytes.iter().position(|b| *b == 0).unwrap_or(NAME_LEN);
        // Nothing but padding may follow the name.
        if bytes[len..].iter().any(|b| *b != 0) {
            return None;
        }
        core::str::from_utf8(&bytes[..len]).ok().and_then(Self::new)
    }

    pub fn to_wire(self) -> [u8; NAME_LEN] {
        self.bytes
    }

    pub fn as_str(&self) -> &str {
        let len = self.bytes.iter().position(|b| *b == 0).unwrap_or(NAME_LEN);
        // Only ASCII gets in.
        core::str::from_utf8(&self.bytes[..len]).unwrap_or("")
    }
}

/// What a sensor is used for by the heating controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensorRole {
    None,
    /// The tank sensor at this position, 0 = top … 6 = bottom.
    Tank(u8),
    /// The flow temperature of the heating circuit.
    Flow,
}

/// Number of tank positions.
pub const TANK_POSITIONS: u8 = 7;

impl SensorRole {
    pub const fn to_byte(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Tank(position) => 1 + position,
            Self::Flow => 1 + TANK_POSITIONS,
        }
    }

    pub const fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(Self::None),
            1..=7 => Some(Self::Tank(byte - 1)),
            8 => Some(Self::Flow),
            _ => None,
        }
    }

    /// `none`, `tank0` … `tank6`, `flow`.
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Tank(0) => "tank0",
            Self::Tank(1) => "tank1",
            Self::Tank(2) => "tank2",
            Self::Tank(3) => "tank3",
            Self::Tank(4) => "tank4",
            Self::Tank(5) => "tank5",
            Self::Tank(_) => "tank6",
            Self::Flow => "flow",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        (0..=8).find_map(|byte| Self::from_byte(byte).filter(|role| role.name() == name))
    }
}

const OP_NAME: u8 = 0;
const OP_INFO: u8 = 1;
const OP_ROLE: u8 = 2;
const OP_FORGET: u8 = 3;

/// One frame of the sensor management.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensorConfig {
    /// Name a sensor (to the node), or the name of a sensor (from it).
    Name { slot: u8, name: SensorName },
    /// Ask for one sensor's info.
    InfoRequest { slot: u8 },
    /// A sensor's info (from the node).
    Info {
        slot: u8,
        serial: [u8; 6],
        lost: bool,
        settling: bool,
        role: SensorRole,
    },
    /// Give a sensor a role. `guard` is the first four bytes of the serial the
    /// sender thinks `slot` has.
    Role {
        slot: u8,
        role: SensorRole,
        guard: [u8; 4],
    },
    /// Forget the sensors that are lost.
    Forget,
}

impl SensorConfig {
    pub fn to_payload(&self) -> Result<Payload, EncodeError> {
        let check = |slot: u8| {
            if slot < MAX_SLOTS {
                Ok(slot)
            } else {
                Err(EncodeError)
            }
        };
        let mut b = [0u8; 8];
        let len = match *self {
            Self::Name { slot, name } => {
                b[0] = check(slot)? | (OP_NAME << 5);
                b[1..8].copy_from_slice(&name.to_wire());
                8
            }
            Self::InfoRequest { slot } => {
                b[0] = check(slot)? | (OP_INFO << 5);
                1
            }
            Self::Info {
                slot,
                serial,
                lost,
                settling,
                role,
            } => {
                b[0] = check(slot)? | (OP_INFO << 5);
                b[1..7].copy_from_slice(&serial);
                b[7] = u8::from(lost) | (u8::from(settling) << 1) | (role.to_byte() << 4);
                8
            }
            Self::Role { slot, role, guard } => {
                b[0] = check(slot)? | (OP_ROLE << 5);
                b[1] = role.to_byte();
                b[2..6].copy_from_slice(&guard);
                6
            }
            Self::Forget => {
                b[0] = OP_FORGET << 5;
                1
            }
        };
        Ok(Payload::new(b, len))
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, DecodeError> {
        let first = *data.first().ok_or(DecodeError)?;
        let slot = first & 0b1_1111;
        let need = |n: usize| {
            if data.len() >= n {
                Ok(())
            } else {
                Err(DecodeError)
            }
        };
        Ok(match first >> 5 {
            OP_NAME => {
                need(8)?;
                let wire: [u8; NAME_LEN] = data[1..8].try_into().map_err(|_| DecodeError)?;
                Self::Name {
                    slot,
                    name: SensorName::from_wire(&wire).ok_or(DecodeError)?,
                }
            }
            OP_INFO if data.len() < 8 => Self::InfoRequest { slot },
            OP_INFO => Self::Info {
                slot,
                serial: data[1..7].try_into().map_err(|_| DecodeError)?,
                lost: data[7] & 1 != 0,
                settling: data[7] & 2 != 0,
                role: SensorRole::from_byte(data[7] >> 4).ok_or(DecodeError)?,
            },
            OP_ROLE => {
                need(6)?;
                Self::Role {
                    slot,
                    role: SensorRole::from_byte(data[1]).ok_or(DecodeError)?,
                    guard: data[2..6].try_into().map_err(|_| DecodeError)?,
                }
            }
            OP_FORGET => Self::Forget,
            _ => return Err(DecodeError),
        })
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    fn name(text: &str) -> SensorName {
        SensorName::new(text).unwrap()
    }

    #[test]
    fn names_are_checked() {
        assert!(SensorName::new("TANK-O1").is_some());
        assert!(SensorName::new("a").is_some());
        assert!(SensorName::new("x_1-Y").is_some());
        for bad in ["", "TOOLONG1", "a b", "a/b", "a+b", "a#", "ä", "tank.1"] {
            assert!(SensorName::new(bad).is_none(), "{bad:?}");
        }
        assert_eq!(name("TANK-O1").as_str(), "TANK-O1");
        assert_eq!(name("VL").as_str(), "VL");
    }

    #[test]
    fn names_on_the_wire_are_nul_padded_and_strict() {
        assert_eq!(name("VL").to_wire(), *b"VL\0\0\0\0\0");
        assert_eq!(name("TANK-O1").to_wire(), *b"TANK-O1");
        assert_eq!(SensorName::from_wire(b"VL\0\0\0\0\0"), Some(name("VL")));
        assert_eq!(SensorName::from_wire(b"TANK-O1"), Some(name("TANK-O1")));
        // Empty, or something after the padding, or not allowed characters.
        assert_eq!(SensorName::from_wire(&[0; 7]), None);
        assert_eq!(SensorName::from_wire(b"V\0L\0\0\0\0"), None);
        assert_eq!(SensorName::from_wire(b"A B\0\0\0\0"), None);
    }

    #[test]
    fn roles_roundtrip_by_byte_and_name() {
        for byte in 0..=8 {
            let role = SensorRole::from_byte(byte).unwrap();
            assert_eq!(role.to_byte(), byte);
            assert_eq!(SensorRole::from_name(role.name()), Some(role));
        }
        assert_eq!(SensorRole::from_byte(9), None);
        assert_eq!(SensorRole::from_name("tank7"), None);
        assert_eq!(SensorRole::from_name("Flow"), None);
        assert_eq!(SensorRole::Tank(0).name(), "tank0");
        assert_eq!(SensorRole::Tank(6).name(), "tank6");
        assert_eq!(SensorRole::Flow.to_byte(), 8);
    }

    fn bytes(message: SensorConfig) -> std::vec::Vec<u8> {
        message.to_payload().unwrap().as_slice().to_vec()
    }

    #[test]
    fn name_layout() {
        let message = SensorConfig::Name {
            slot: 5,
            name: name("TANK-O1"),
        };
        assert_eq!(
            bytes(message),
            [5, b'T', b'A', b'N', b'K', b'-', b'O', b'1']
        );
        assert_eq!(SensorConfig::from_bytes(&bytes(message)), Ok(message));
    }

    #[test]
    fn info_request_and_info_layout() {
        let request = SensorConfig::InfoRequest { slot: 31 };
        assert_eq!(bytes(request), [0b0011_1111]);
        assert_eq!(SensorConfig::from_bytes(&bytes(request)), Ok(request));

        let info = SensorConfig::Info {
            slot: 2,
            serial: [0x22, 0xB0, 0xC3, 0x32, 0x20, 0x01],
            lost: false,
            settling: true,
            role: SensorRole::Tank(2),
        };
        // slot 2, op 1; flags: settling; role byte 3 in bits 4-7.
        assert_eq!(
            bytes(info),
            [0b0010_0010, 0x22, 0xB0, 0xC3, 0x32, 0x20, 0x01, 0b0011_0010]
        );
        assert_eq!(SensorConfig::from_bytes(&bytes(info)), Ok(info));
    }

    #[test]
    fn role_layout() {
        let message = SensorConfig::Role {
            slot: 7,
            role: SensorRole::Flow,
            guard: [1, 2, 3, 4],
        };
        // slot 7, op 2 -> 0b010_00111.
        assert_eq!(bytes(message), [0b0100_0111, 8, 1, 2, 3, 4]);
        assert_eq!(SensorConfig::from_bytes(&bytes(message)), Ok(message));
    }

    #[test]
    fn forget_layout() {
        assert_eq!(bytes(SensorConfig::Forget), [0b0110_0000]);
        assert_eq!(
            SensorConfig::from_bytes(&[0b0110_0000]),
            Ok(SensorConfig::Forget)
        );
    }

    #[test]
    fn slots_above_31_cannot_be_sent() {
        let message = SensorConfig::InfoRequest { slot: 32 };
        assert_eq!(message.to_payload(), Err(EncodeError));
    }

    #[test]
    fn bad_frames_are_rejected() {
        assert_eq!(SensorConfig::from_bytes(&[]), Err(DecodeError));
        // name too short, unknown op, role short or with an unknown role byte
        assert_eq!(SensorConfig::from_bytes(&[0, b'A']), Err(DecodeError));
        assert_eq!(SensorConfig::from_bytes(&[0b1000_0000]), Err(DecodeError));
        assert_eq!(
            SensorConfig::from_bytes(&[0b0100_0000, 1, 0, 0]),
            Err(DecodeError)
        );
        assert_eq!(
            SensorConfig::from_bytes(&[0b0100_0000, 9, 0, 0, 0, 0]),
            Err(DecodeError)
        );
        // info with an unknown role in the flags byte
        assert_eq!(
            SensorConfig::from_bytes(&[0b0010_0000, 0, 0, 0, 0, 0, 0, 0b1111_0000]),
            Err(DecodeError)
        );
    }
}
