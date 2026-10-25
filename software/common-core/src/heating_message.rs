//! Wire format of the heating-controller messages, sent to a `HeatingControl`
//! node: the command frame ([`CanMessageType::HeatingCommand`]) and the state
//! it answers to an R frame ([`CanMessageType::HeatingState`]).
//!
//! Everything the heating is told goes into one message type: the first byte
//! holds an operation code (bits 0-2) and, in the bits above it, that
//! operation's flags; the remaining bytes the values. Temperatures are the
//! same signed 1/16 °C fixed-point values the `TemperatureSensor` messages
//! use, multi-byte values are little-endian, extra bytes in a frame are
//! ignored.
//!
//! | op | command | byte 0 bits 3-7 | bytes 1.. |
//! |---|---|---|---|
//! | 0 | [`HeatingTarget`] | bit 3: tank mode | flow target `i16` |
//! | 1 | [`HeatingEnergy`] | bit 3: rod released | power `i32` W |
//! | 2 | [`HeatingEmergency`] | bits 3-4: rod stage | tank target `i16` |
//! | 3 | [`HeatingManual`] | bit 3: manual mode on, bit 4: relay given, bits 5-7: relay − 6 | bit 0: relay on |
//!
//! [`CanMessageType::HeatingCommand`]: crate::can_message_type::CanMessageType::HeatingCommand
//! [`CanMessageType::HeatingState`]: crate::can_message_type::CanMessageType::HeatingState

use crate::{DecodeError, EncodeError, Payload};

/// Converts a 1/16 °C fixed-point value to milli-degrees Celsius.
pub const fn raw_to_millicelsius(raw: i16) -> i32 {
    raw as i32 * 125 / 2
}

/// Converts milli-degrees Celsius to the 1/16 °C fixed-point value,
/// rounding to nearest and saturating at the `i16` range.
pub const fn millicelsius_to_raw(millicelsius: i32) -> i16 {
    // Wide enough that the doubling can't overflow for any input.
    let m = millicelsius as i64;
    let raw = if m >= 0 {
        (m * 2 + 62) / 125
    } else {
        (m * 2 - 62) / 125
    };
    if raw > i16::MAX as i64 {
        i16::MAX
    } else if raw < i16::MIN as i64 {
        i16::MIN
    } else {
        raw as i16
    }
}

/// What the heating circuit should do. Flag: supply the circuit from the
/// tank instead of directly from the heat pump.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeatingTarget {
    /// Wanted flow temperature of the heating circuit, in 1/16 °C.
    pub flow_target_raw: i16,
    pub tank_mode: bool,
}

impl HeatingTarget {
    pub fn flow_target_millicelsius(&self) -> i32 {
        raw_to_millicelsius(self.flow_target_raw)
    }
}

/// Available power for the tank's heating rod. Flag: the rod is released at
/// all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeatingEnergy {
    /// Available (feed-in) power in W; negative while importing.
    pub power_w: i32,
    pub rod_enabled: bool,
}

/// Emergency operation: while these keep arriving, the rod runs at `stage`
/// (0-3) regardless of the available power, regulating the tank to
/// `tank_target`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeatingEmergency {
    pub stage: u8,
    pub tank_target_raw: i16,
}

impl HeatingEmergency {
    pub fn tank_target_millicelsius(&self) -> i32 {
        raw_to_millicelsius(self.tank_target_raw)
    }
}

/// Takes over the relays 6-10 (valves, mixer, pump) of the heating
/// controller by hand, for commissioning. With `enabled` the controller
/// leaves those relays alone (they all go off when it starts), without it the
/// controller has them back. `relay` (6-10) and `relay_on` switch one of them,
/// in manual mode only. The heating rod (relays 0-5) is never reachable this
/// way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeatingManual {
    /// The manual mode is on.
    pub enabled: bool,
    /// The relay to switch, if any.
    pub relay: Option<u8>,
    /// Whether that relay is switched on.
    pub relay_on: bool,
}

/// First relay of the manual mode.
pub const FIRST_MANUAL_RELAY: u8 = 6;
/// Last relay of the manual mode.
pub const LAST_MANUAL_RELAY: u8 = 10;

const OP_MASK: u8 = 0b111;
const OP_TARGET: u8 = 0;
const OP_ENERGY: u8 = 1;
const OP_EMERGENCY: u8 = 2;
const OP_MANUAL: u8 = 3;

/// One command to the heating controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeatingCommand {
    Target(HeatingTarget),
    Energy(HeatingEnergy),
    Emergency(HeatingEmergency),
    Manual(HeatingManual),
}

impl HeatingCommand {
    /// The frame's data bytes. Fails for values the format can't hold: a
    /// rod stage above 3, or a manual relay outside 6-10.
    pub fn to_payload(&self) -> Result<Payload, EncodeError> {
        let mut b = [0u8; 8];
        let len = match *self {
            Self::Target(t) => {
                b[0] = OP_TARGET | (u8::from(t.tank_mode) << 3);
                b[1..3].copy_from_slice(&t.flow_target_raw.to_le_bytes());
                3
            }
            Self::Energy(e) => {
                b[0] = OP_ENERGY | (u8::from(e.rod_enabled) << 3);
                b[1..5].copy_from_slice(&e.power_w.to_le_bytes());
                5
            }
            Self::Emergency(e) => {
                if e.stage > 3 {
                    return Err(EncodeError);
                }
                b[0] = OP_EMERGENCY | (e.stage << 3);
                b[1..3].copy_from_slice(&e.tank_target_raw.to_le_bytes());
                3
            }
            Self::Manual(m) => {
                b[0] = OP_MANUAL | (u8::from(m.enabled) << 3);
                if let Some(relay) = m.relay {
                    if !(FIRST_MANUAL_RELAY..=LAST_MANUAL_RELAY).contains(&relay) {
                        return Err(EncodeError);
                    }
                    b[0] |= (1 << 4) | ((relay - FIRST_MANUAL_RELAY) << 5);
                    b[1] = u8::from(m.relay_on);
                    2
                } else {
                    1
                }
            }
        };
        Ok(Payload::new(b, len))
    }

    /// Decodes a frame's data bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self, DecodeError> {
        let first = *data.first().ok_or(DecodeError)?;
        let need = |n: usize| {
            if data.len() >= n {
                Ok(())
            } else {
                Err(DecodeError)
            }
        };
        Ok(match first & OP_MASK {
            OP_TARGET => {
                need(3)?;
                Self::Target(HeatingTarget {
                    flow_target_raw: i16::from_le_bytes([data[1], data[2]]),
                    tank_mode: first & (1 << 3) != 0,
                })
            }
            OP_ENERGY => {
                need(5)?;
                Self::Energy(HeatingEnergy {
                    power_w: i32::from_le_bytes([data[1], data[2], data[3], data[4]]),
                    rod_enabled: first & (1 << 3) != 0,
                })
            }
            OP_EMERGENCY => {
                need(3)?;
                Self::Emergency(HeatingEmergency {
                    stage: (first >> 3) & 0b11,
                    tank_target_raw: i16::from_le_bytes([data[1], data[2]]),
                })
            }
            OP_MANUAL => {
                let relay_given = first & (1 << 4) != 0;
                let relay = if relay_given {
                    need(2)?;
                    let relay = FIRST_MANUAL_RELAY + (first >> 5);
                    // 3 bits reach to 13: only 6-10 are relays of the mode.
                    if relay > LAST_MANUAL_RELAY {
                        return Err(DecodeError);
                    }
                    Some(relay)
                } else {
                    None
                };
                Self::Manual(HeatingManual {
                    enabled: first & (1 << 3) != 0,
                    relay,
                    relay_on: relay_given && data[1] & 1 != 0,
                })
            }
            _ => return Err(DecodeError),
        })
    }
}

/// What the controller answers to an R frame
/// ([`CanMessageType::HeatingState`](crate::can_message_type::CanMessageType::HeatingState)).
///
/// Wire layout (2 bytes). Byte 0: bit 0 manual mode on, bits 1-3 the
/// target/energy/emergency input is alive (arrived within the last 30 s),
/// bit 4 over-temperature latch, bits 5-6 rod stage. Byte 1: relays 6-10 that
/// are on in manual mode, bit 0 = relay 6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeatingState {
    pub manual: bool,
    pub target_alive: bool,
    pub energy_alive: bool,
    pub emergency_alive: bool,
    pub over_temperature: bool,
    pub rod_stage: u8,
    pub manual_relays_on: u8,
}

impl HeatingState {
    pub fn to_payload(&self) -> Payload {
        let mut b = [0u8; 8];
        b[0] = u8::from(self.manual)
            | (u8::from(self.target_alive) << 1)
            | (u8::from(self.energy_alive) << 2)
            | (u8::from(self.emergency_alive) << 3)
            | (u8::from(self.over_temperature) << 4)
            | ((self.rod_stage & 0b11) << 5);
        b[1] = self.manual_relays_on & 0b1_1111;
        Payload::new(b, 2)
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, DecodeError> {
        if data.len() < 2 {
            return Err(DecodeError);
        }
        Ok(Self {
            manual: data[0] & 1 != 0,
            target_alive: data[0] & (1 << 1) != 0,
            energy_alive: data[0] & (1 << 2) != 0,
            emergency_alive: data[0] & (1 << 3) != 0,
            over_temperature: data[0] & (1 << 4) != 0,
            rod_stage: (data[0] >> 5) & 0b11,
            manual_relays_on: data[1] & 0b1_1111,
        })
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    fn bytes(command: HeatingCommand) -> std::vec::Vec<u8> {
        command.to_payload().unwrap().as_slice().to_vec()
    }

    #[test]
    fn target_layout() {
        // 45.5 °C = 728 = 0x02D8, tank mode on.
        let command = HeatingCommand::Target(HeatingTarget {
            flow_target_raw: 728,
            tank_mode: true,
        });
        assert_eq!(bytes(command), [0b0000_1000, 0xD8, 0x02]);
        assert_eq!(HeatingCommand::from_bytes(&bytes(command)), Ok(command));
        let off = HeatingCommand::Target(HeatingTarget {
            flow_target_raw: -16,
            tank_mode: false,
        });
        assert_eq!(bytes(off), [0, 0xF0, 0xFF]);
        assert_eq!(HeatingCommand::from_bytes(&bytes(off)), Ok(off));
    }

    #[test]
    fn energy_layout_including_negative_power() {
        let command = HeatingCommand::Energy(HeatingEnergy {
            power_w: 6200,
            rod_enabled: true,
        });
        assert_eq!(bytes(command), [0b0000_1001, 0x38, 0x18, 0, 0]);
        assert_eq!(HeatingCommand::from_bytes(&bytes(command)), Ok(command));
        let import = HeatingCommand::Energy(HeatingEnergy {
            power_w: -1500,
            rod_enabled: false,
        });
        assert_eq!(HeatingCommand::from_bytes(&bytes(import)), Ok(import));
        assert_eq!(bytes(import)[0], 0b0000_0001);
    }

    #[test]
    fn emergency_layout_and_stage_limit() {
        let command = HeatingCommand::Emergency(HeatingEmergency {
            stage: 3,
            tank_target_raw: 880, // 55 °C
        });
        assert_eq!(bytes(command), [0b0001_1010, 0x70, 0x03]);
        assert_eq!(HeatingCommand::from_bytes(&bytes(command)), Ok(command));
        let too_high = HeatingCommand::Emergency(HeatingEmergency {
            stage: 4,
            tank_target_raw: 0,
        });
        assert_eq!(too_high.to_payload(), Err(EncodeError));
    }

    #[test]
    fn manual_layout() {
        let mode_only = HeatingCommand::Manual(HeatingManual {
            enabled: true,
            relay: None,
            relay_on: false,
        });
        assert_eq!(bytes(mode_only), [0b0000_1011]);
        assert_eq!(HeatingCommand::from_bytes(&bytes(mode_only)), Ok(mode_only));

        let relay = HeatingCommand::Manual(HeatingManual {
            enabled: true,
            relay: Some(10),
            relay_on: true,
        });
        // op 3, mode on (bit 3), relay given (bit 4), relay 10 - 6 = 4 (bits 5-7).
        assert_eq!(bytes(relay), [0b1001_1011, 1]);
        assert_eq!(HeatingCommand::from_bytes(&bytes(relay)), Ok(relay));

        let off = HeatingCommand::Manual(HeatingManual {
            enabled: false,
            relay: None,
            relay_on: false,
        });
        assert_eq!(bytes(off), [0b0000_0011]);
    }

    #[test]
    fn manual_relays_outside_6_to_10_are_refused_both_ways() {
        for relay in [0u8, 5, 11, 255] {
            let command = HeatingCommand::Manual(HeatingManual {
                enabled: true,
                relay: Some(relay),
                relay_on: true,
            });
            assert_eq!(command.to_payload(), Err(EncodeError));
        }
        // Relay index 5-7 does fit in the 3 bits but is no relay of the mode.
        assert_eq!(
            HeatingCommand::from_bytes(&[0b1011_1011, 1]),
            Err(DecodeError)
        );
    }

    #[test]
    fn a_relay_that_is_not_given_is_not_switched() {
        // Garbage in the relay bits and byte 1 without the "given" bit.
        let command = HeatingCommand::from_bytes(&[0b1110_1011, 1]).unwrap();
        assert_eq!(
            command,
            HeatingCommand::Manual(HeatingManual {
                enabled: true,
                relay: None,
                relay_on: false
            })
        );
    }

    #[test]
    fn short_frames_unknown_ops_and_unused_flag_bits() {
        assert_eq!(HeatingCommand::from_bytes(&[]), Err(DecodeError));
        assert_eq!(HeatingCommand::from_bytes(&[0, 0]), Err(DecodeError));
        assert_eq!(HeatingCommand::from_bytes(&[1, 0, 0, 0]), Err(DecodeError));
        assert_eq!(HeatingCommand::from_bytes(&[2, 0]), Err(DecodeError));
        assert_eq!(HeatingCommand::from_bytes(&[0b1001_1011]), Err(DecodeError));
        for op in 4..=7 {
            assert_eq!(
                HeatingCommand::from_bytes(&[op, 0, 0, 0, 0]),
                Err(DecodeError)
            );
        }
        // Extra bytes and the target's unused flag bits are ignored.
        let command = HeatingCommand::from_bytes(&[0b1111_1000, 0, 0, 9, 9]).unwrap();
        assert_eq!(
            command,
            HeatingCommand::Target(HeatingTarget {
                flow_target_raw: 0,
                tank_mode: true
            })
        );
    }

    #[test]
    fn state_layout() {
        let state = HeatingState {
            manual: true,
            target_alive: false,
            energy_alive: true,
            emergency_alive: false,
            over_temperature: true,
            rod_stage: 2,
            manual_relays_on: 0b1_0100,
        };
        assert_eq!(state.to_payload().as_slice(), [0b0101_0101, 0b1_0100]);
        assert_eq!(
            HeatingState::from_bytes(state.to_payload().as_slice()),
            Ok(state)
        );
        assert_eq!(HeatingState::from_bytes(&[1]), Err(DecodeError));
    }

    #[test]
    fn state_ignores_unknown_relay_bits() {
        let state = HeatingState::from_bytes(&[0, 0xFF]).unwrap();
        assert_eq!(state.manual_relays_on, 0b1_1111);
        assert!(!state.manual);
    }

    #[test]
    fn temperature_conversions() {
        assert_eq!(millicelsius_to_raw(45_500), 728);
        assert_eq!(raw_to_millicelsius(728), 45_500);
        assert_eq!(millicelsius_to_raw(-3_250), -52);
        assert_eq!(millicelsius_to_raw(i32::MAX), i16::MAX);
        assert_eq!(millicelsius_to_raw(i32::MIN), i16::MIN);
    }
}
