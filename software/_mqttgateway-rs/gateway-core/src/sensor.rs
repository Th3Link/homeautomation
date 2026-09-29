//! Decoding for the fixed-point environmental sensor messages
//! (`TemperatureSensor`, `PressureSensor`, `HumiditySensor`,
//! `Co2Equivalent`, `VocBreath`, `AirQuality`) and the raw
//! `AmbientLightSensor`/`AmbientLightSensorWhite` payload.

/// A decoded fixed-point sensor reading.
///
/// Wire layout (8 bytes): `[sub_id(LE 48-bit), raw_value(LE 16-bit)]`.
/// `value()` applies the `/16.0` fixed-point scale the original C++ uses
/// for every sensor of this shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FixedPointReading {
    /// The low 48 bits of an accompanying identifier (e.g. a sub-sensor
    /// index), carried alongside the value on the same frame.
    pub sub_id: u64,
    raw_value: u16,
}

impl FixedPointReading {
    /// The decoded, human-scale reading (`raw_value / 16.0`).
    pub fn value(self) -> f64 {
        f64::from(self.raw_value) / 16.0
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, crate::DecodeError> {
        if data.len() < 8 {
            return Err(crate::DecodeError);
        }
        let sub_id =
            u64::from_le_bytes([data[0], data[1], data[2], data[3], data[4], data[5], 0, 0]);
        let raw_value = u16::from_le_bytes([data[6], data[7]]);
        Ok(Self { sub_id, raw_value })
    }
}

/// Decodes an `AmbientLightSensor`/`AmbientLightSensorWhite` payload: a
/// raw little-endian `u32`, unscaled (the original C++ divides by `1.0`,
/// i.e. does nothing).
///
/// Wire layout (4 bytes): `[value(LE 32-bit)]`.
pub fn decode_ambient_light(data: &[u8]) -> Result<u32, crate::DecodeError> {
    if data.len() < 4 {
        return Err(crate::DecodeError);
    }
    Ok(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_point_applies_sixteenth_scale() {
        // 16 raw = 1.0 scaled, matching the original's `/16.0`.
        let data = [0, 0, 0, 0, 0, 0, 16, 0];
        let reading = FixedPointReading::from_bytes(&data).unwrap();
        assert_eq!(reading.value(), 1.0);
    }

    #[test]
    fn fixed_point_decodes_48_bit_sub_id() {
        let data = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0, 0];
        let reading = FixedPointReading::from_bytes(&data).unwrap();
        assert_eq!(reading.sub_id, 0x0000_0605_0403_0201);
    }

    #[test]
    fn fixed_point_rejects_short_input() {
        assert!(FixedPointReading::from_bytes(&[0u8; 7]).is_err());
    }

    #[test]
    fn ambient_light_is_unscaled_raw_u32() {
        assert_eq!(
            decode_ambient_light(&[0x78, 0x56, 0x34, 0x12]).unwrap(),
            0x1234_5678
        );
    }

    #[test]
    fn ambient_light_rejects_short_input() {
        assert!(decode_ambient_light(&[0u8; 3]).is_err());
    }
}
