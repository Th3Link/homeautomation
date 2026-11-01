//! The sensor extension board of the light switch: what its BME680 and its
//! night light (a PCF8574A) put on the wire.
//!
//! The BME680's readings go out in the fixed-point sensor layout the
//! DS18B20s use (`[sub_id u48 LE, raw u16 LE]`, value = raw / 16), as
//! `TemperatureSensor` (°C, signed), `HumiditySensor` (%) and
//! `PressureSensor` (hPa). Their `sub_id` is [`bme680_sub_id`]: stable per
//! device, and distinct from any DS18B20 serial.

pub use common_core::sensor_id::bme680 as bme680_sub_id;

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

/// The BME680's heater set point for one forced measurement, as BSEC asks
/// for it: `res_heat_0` (register 0x5A) for a target temperature and
/// `gas_wait_0` (0x64) for a duration. The calibration values come from the
/// chip (`par_gh1`-`par_gh3`, `res_heat_range`, `res_heat_val`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeaterCalibration {
    pub par_gh1: i8,
    pub par_gh2: i16,
    pub par_gh3: i8,
    pub res_heat_range: u8,
    pub res_heat_val: i8,
}

/// Register of the heater's resistance set point, profile step 0.
pub const RES_HEAT_0: u8 = 0x5A;
/// Register of the heater's duration, profile step 0.
pub const GAS_WAIT_0: u8 = 0x64;

impl HeaterCalibration {
    /// `res_heat_0` for `target` °C (capped at 400) at `ambient` °C:
    /// Bosch's integer formula (BME680 datasheet 5.3.3.3).
    pub fn res_heat(&self, target: u16, ambient: i32) -> u8 {
        let target = i32::from(target.min(400));
        let var1 = ((ambient * i32::from(self.par_gh3)) / 1000) * 256;
        let var2 = (i32::from(self.par_gh1) + 784)
            * (((((i32::from(self.par_gh2) + 154_009) * target * 5) / 100) + 3_276_800) / 10);
        let var3 = var1 + (var2 / 2);
        let var4 = var3 / (i32::from(self.res_heat_range) + 4);
        let var5 = (131 * i32::from(self.res_heat_val)) + 65_536;
        let res_heat_x100 = ((var4 / var5) - 250) * 34;
        ((res_heat_x100 + 50) / 100) as u8
    }
}

/// `gas_wait_0` for `ms` milliseconds: 6 bits of value and a factor of 1,
/// 4, 16 or 64; 4,032 ms and more is the maximum.
pub fn gas_wait(ms: u16) -> u8 {
    if ms >= 0xFC0 {
        return 0xFF;
    }
    let mut duration = ms;
    let mut factor = 0u8;
    while duration > 0x3F {
        duration /= 4;
        factor += 1;
    }
    duration as u8 + factor * 64
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

    fn calibration() -> HeaterCalibration {
        // A BME680's values (the test board's order of magnitude).
        HeaterCalibration {
            par_gh1: -30,
            par_gh2: -12_000,
            par_gh3: 18,
            res_heat_range: 1,
            res_heat_val: 40,
        }
    }

    #[test]
    fn heater_matches_the_datasheets_float_formula() {
        // BME680 datasheet 3.3.5, the floating-point version.
        let c = calibration();
        for (target, ambient) in [(300, 21), (320, 21), (320, -10), (200, 35)] {
            let var1 = f64::from(c.par_gh1) / 16.0 + 49.0;
            let var2 = f64::from(c.par_gh2) / 32768.0 * 0.0005 + 0.00235;
            let var3 = f64::from(c.par_gh3) / 1024.0;
            let var4 = var1 * (1.0 + var2 * f64::from(target));
            let var5 = var4 + var3 * f64::from(ambient);
            let float = 3.4
                * (var5
                    * (4.0 / (4.0 + f64::from(c.res_heat_range)))
                    * (1.0 / (1.0 + f64::from(c.res_heat_val) * 0.002))
                    - 25.0);
            let int = f64::from(c.res_heat(target, ambient));
            // Bosch's integer version rounds down along the way.
            assert!((int - float).abs() <= 2.0, "{target} °C: {int} vs {float}");
        }
    }

    #[test]
    fn heater_set_points() {
        let c = calibration();
        // Hotter needs more resistance; 400 °C is the cap.
        assert!(c.res_heat(320, 25) > c.res_heat(300, 25));
        assert_eq!(c.res_heat(450, 25), c.res_heat(400, 25));
        assert_eq!(gas_wait(63), 63);
        assert_eq!(gas_wait(64), 64 + 16);
        assert_eq!(gas_wait(150), 64 + 37);
        assert_eq!(gas_wait(5000), 0xFF);
    }

    #[test]
    fn nightlight_levels() {
        assert_eq!(nightlight_port(0), 0xFF);
        assert_eq!(nightlight_port(3), 0xF8);
        assert_eq!(nightlight_port(9), 0xFF);
    }
}
