//! Wire format of the three heating-controller messages
//! ([`CanMessageType::HeatingTarget`], [`CanMessageType::HeatingEnergy`],
//! [`CanMessageType::HeatingEmergency`]), sent to a `HeatingControl` node.
//!
//! Temperatures are the same signed 1/16 °C fixed-point values the
//! `TemperatureSensor` messages use. Multi-byte values are little-endian.
//! Frames may carry more bytes than listed; the extra bytes are ignored.
//!
//! [`CanMessageType::HeatingTarget`]: crate::can_message_type::CanMessageType::HeatingTarget
//! [`CanMessageType::HeatingEnergy`]: crate::can_message_type::CanMessageType::HeatingEnergy
//! [`CanMessageType::HeatingEmergency`]: crate::can_message_type::CanMessageType::HeatingEmergency

use crate::DecodeError;

/// Converts a 1/16 °C fixed-point value to milli-degrees Celsius.
pub const fn raw_to_millicelsius(raw: i16) -> i32 {
    raw as i32 * 125 / 2
}

/// Converts milli-degrees Celsius to the 1/16 °C fixed-point value,
/// rounding to nearest and saturating at the `i16` range.
pub const fn millicelsius_to_raw(millicelsius: i32) -> i16 {
    let raw = if millicelsius >= 0 {
        (millicelsius * 2 + 62) / 125
    } else {
        (millicelsius * 2 - 62) / 125
    };
    if raw > i16::MAX as i32 {
        i16::MAX
    } else if raw < i16::MIN as i32 {
        i16::MIN
    } else {
        raw as i16
    }
}

/// What the heating circuit should do: [`HeatingTarget`](crate::can_message_type::CanMessageType::HeatingTarget).
///
/// Wire layout (3 bytes): `[flow_target(i16, 1/16 °C), flags]`, where flag
/// bit 0 requests tank operation (heating circuit fed from the tank
/// instead of directly from the heat pump).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeatingTarget {
    /// Wanted flow temperature of the heating circuit, in 1/16 °C.
    pub flow_target_raw: i16,
    pub tank_mode: bool,
}

impl HeatingTarget {
    pub fn from_bytes(data: &[u8]) -> Result<Self, DecodeError> {
        if data.len() < 3 {
            return Err(DecodeError);
        }
        Ok(Self {
            flow_target_raw: i16::from_le_bytes([data[0], data[1]]),
            tank_mode: data[2] & 1 != 0,
        })
    }

    pub fn to_bytes(&self) -> [u8; 3] {
        let t = self.flow_target_raw.to_le_bytes();
        [t[0], t[1], u8::from(self.tank_mode)]
    }

    pub fn flow_target_millicelsius(&self) -> i32 {
        raw_to_millicelsius(self.flow_target_raw)
    }
}

/// Available power for the tank's heating rod:
/// [`HeatingEnergy`](crate::can_message_type::CanMessageType::HeatingEnergy).
///
/// Wire layout (5 bytes): `[power(i32, W), flags]`, where flag bit 0
/// releases the heating rod at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeatingEnergy {
    /// Surplus (feed-in) power in watts; negative while importing.
    pub power_w: i32,
    pub rod_enabled: bool,
}

impl HeatingEnergy {
    pub fn from_bytes(data: &[u8]) -> Result<Self, DecodeError> {
        if data.len() < 5 {
            return Err(DecodeError);
        }
        Ok(Self {
            power_w: i32::from_le_bytes([data[0], data[1], data[2], data[3]]),
            rod_enabled: data[4] & 1 != 0,
        })
    }

    pub fn to_bytes(&self) -> [u8; 5] {
        let p = self.power_w.to_le_bytes();
        [p[0], p[1], p[2], p[3], u8::from(self.rod_enabled)]
    }
}

/// Emergency operation:
/// [`HeatingEmergency`](crate::can_message_type::CanMessageType::HeatingEmergency).
///
/// Wire layout (3 bytes): `[stage, tank_target(i16, 1/16 °C)]`. While these
/// keep arriving, the rod runs at `stage` (0-3) regardless of the available
/// power, regulating the tank to `tank_target`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeatingEmergency {
    pub stage: u8,
    pub tank_target_raw: i16,
}

impl HeatingEmergency {
    pub fn from_bytes(data: &[u8]) -> Result<Self, DecodeError> {
        if data.len() < 3 {
            return Err(DecodeError);
        }
        Ok(Self {
            stage: data[0],
            tank_target_raw: i16::from_le_bytes([data[1], data[2]]),
        })
    }

    pub fn to_bytes(&self) -> [u8; 3] {
        let t = self.tank_target_raw.to_le_bytes();
        [self.stage, t[0], t[1]]
    }

    pub fn tank_target_millicelsius(&self) -> i32 {
        raw_to_millicelsius(self.tank_target_raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_roundtrips_and_pins_the_layout() {
        // 45.5 °C = 728 = 0x02D8, tank mode on.
        let msg = HeatingTarget {
            flow_target_raw: 728,
            tank_mode: true,
        };
        assert_eq!(msg.to_bytes(), [0xD8, 0x02, 0x01]);
        assert_eq!(HeatingTarget::from_bytes(&msg.to_bytes()), Ok(msg));
        assert_eq!(msg.flow_target_millicelsius(), 45_500);
    }

    #[test]
    fn target_ignores_unknown_flag_bits_and_extra_bytes() {
        let msg = HeatingTarget::from_bytes(&[0, 0, 0b1111_1110, 9, 9]).unwrap();
        assert!(!msg.tank_mode);
    }

    #[test]
    fn energy_roundtrips_including_negative_power() {
        let msg = HeatingEnergy {
            power_w: -1500,
            rod_enabled: true,
        };
        assert_eq!(HeatingEnergy::from_bytes(&msg.to_bytes()), Ok(msg));
        let msg = HeatingEnergy {
            power_w: 6200,
            rod_enabled: false,
        };
        assert_eq!(msg.to_bytes(), [0x38, 0x18, 0, 0, 0]);
        assert_eq!(HeatingEnergy::from_bytes(&msg.to_bytes()), Ok(msg));
    }

    #[test]
    fn emergency_roundtrips_and_pins_the_layout() {
        // Stage 2, tank target 60 °C = 960 = 0x03C0.
        let msg = HeatingEmergency {
            stage: 2,
            tank_target_raw: 960,
        };
        assert_eq!(msg.to_bytes(), [2, 0xC0, 0x03]);
        assert_eq!(HeatingEmergency::from_bytes(&msg.to_bytes()), Ok(msg));
        assert_eq!(msg.tank_target_millicelsius(), 60_000);
    }

    #[test]
    fn short_frames_are_rejected_instead_of_panicking() {
        assert!(HeatingTarget::from_bytes(&[0; 2]).is_err());
        assert!(HeatingEnergy::from_bytes(&[0; 4]).is_err());
        assert!(HeatingEmergency::from_bytes(&[0; 2]).is_err());
    }

    #[test]
    fn millicelsius_conversion_roundtrips_exact_values() {
        for raw in [-1600i16, -162, 0, 1, 16, 728, 960, 1280] {
            assert_eq!(millicelsius_to_raw(raw_to_millicelsius(raw)), raw);
        }
    }

    #[test]
    fn millicelsius_conversion_rounds_and_saturates() {
        // 60 °C is exactly 960; 60.02 °C rounds to the same step.
        assert_eq!(millicelsius_to_raw(60_000), 960);
        assert_eq!(millicelsius_to_raw(60_020), 960);
        assert_eq!(millicelsius_to_raw(-10_125), -162);
        assert_eq!(millicelsius_to_raw(i32::MAX / 4), i16::MAX);
        assert_eq!(millicelsius_to_raw(i32::MIN / 4), i16::MIN);
    }
}
