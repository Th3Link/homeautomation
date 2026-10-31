//! The `sub_id` of a fixed-point sensor reading, where it isn't a DS18B20's
//! serial: the BME680 of a light switch's sensor board.

/// `sub_id` of a BME680 at I2C address `address` (0x76 or 0x77):
/// `0x6800_00xx`, "68" for the chip, the address in the low byte (the
/// gateway names a sensor by the low 32 bits: `0x68000077`).
pub const fn bme680(address: u8) -> u64 {
    0x6800_0000 | address as u64
}

/// Whether `sub_id` is a BME680's. Such a sensor has no slot and name on
/// its node (`SensorConfig` lists only DS18B20s); it is known by its device.
pub const fn is_bme680(sub_id: u64) -> bool {
    sub_id >> 8 == 0x68_0000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bme680_ids() {
        assert_eq!(bme680(0x77), 0x6800_0077);
        assert!(is_bme680(bme680(0x76)));
        // A DS18B20 serial (ROM bytes 1..=6).
        assert!(!is_bme680(0x0120_32c3_b022));
    }
}
