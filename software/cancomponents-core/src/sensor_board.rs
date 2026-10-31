//! The sensor extension board of the light switch: what its BME680 and its
//! night light (a PCF8574A) put on the wire.
//!
//! The BME680's readings go out in the fixed-point sensor layout the
//! DS18B20s use (`[sub_id u48 LE, raw u16 LE]`, value = raw / 16), as
//! `TemperatureSensor` (°C, signed), `HumiditySensor` (%) and
//! `PressureSensor` (hPa). Their `sub_id` is [`bme680_sub_id`]: stable per
//! device, and distinct from any DS18B20 serial.

/// `sub_id` of a BME680 at I2C address `address` (0x76 or 0x77):
/// `0x6800_00xx`, "68" for the chip, the address in the low byte (the
/// gateway names a sensor by the low 32 bits: `0x68000077`).
pub fn bme680_sub_id(address: u8) -> u64 {
    0x6800_0000 | u64::from(address)
}

/// A fixed-point sensor frame.
pub fn frame(sub_id: u64, raw: u16) -> [u8; 8] {
    let id = sub_id.to_le_bytes();
    let raw = raw.to_le_bytes();
    [id[0], id[1], id[2], id[3], id[4], id[5], raw[0], raw[1]]
}

/// °C as 1/16 °C, two's complement.
pub fn temperature_raw(celsius: f32) -> u16 {
    // Rounded half away from zero; `as` saturates.
    let x = celsius * 16.0;
    (if x < 0.0 { x - 0.5 } else { x + 0.5 }) as i16 as u16
}

/// A value in 1/16 of its unit, for the unsigned readings (%, hPa).
pub fn unsigned_raw(value: f32) -> u16 {
    // Rounded; `as` saturates (negative to 0).
    (value * 16.0 + 0.5) as u16
}

/// I2C address of the night light's PCF8574A.
pub const NIGHTLIGHT_ADDRESS: u8 = 0x38;

/// The PCF8574A's port for night light `level` (`Nightlight` message,
/// 0 = off, 1-3 = that many LEDs; the LEDs are on P0-P2, active low).
pub fn nightlight_port(level: u8) -> u8 {
    match level {
        1 => 0xFE,
        2 => 0xFC,
        3 => 0xF8,
        _ => 0xFF,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_use_the_fixed_point_layout() {
        let f = frame(bme680_sub_id(0x77), temperature_raw(21.5));
        assert_eq!(f, [0x77, 0, 0, 0x68, 0, 0, 0x58, 0x01]);
        assert_eq!(temperature_raw(-1.0), 0xFFF0);
        assert_eq!(unsigned_raw(1013.25), 16212);
        assert_eq!(unsigned_raw(-3.0), 0);
    }

    #[test]
    fn nightlight_levels() {
        assert_eq!(nightlight_port(0), 0xFF);
        assert_eq!(nightlight_port(3), 0xF8);
        assert_eq!(nightlight_port(9), 0xFF);
    }
}
