//! The PCA9685 PWM extension (`ExtensionMode::Pwm`): up to 16 boards of 16
//! channels, addressed by the `bank` of a `LampGroup` message (bank 1 is
//! the board at [`BASE_ADDRESS`], bank 2 the next, …), set up the way the
//! original C++ firmware did: 1 kHz, open-drain outputs, the 0-255 lamp
//! value mapped linearly onto 12 bits.

/// I2C address of the first board.
pub const BASE_ADDRESS: u8 = 0x40;
/// Boards (banks 1-16).
pub const BOARDS: u8 = 16;
/// Channels per board.
pub const CHANNELS: usize = 16;
/// PWM frequency.
pub const FREQUENCY_HZ: u32 = 1000;

pub const REG_MODE1: u8 = 0x00;
pub const REG_MODE2: u8 = 0x01;
/// `LED0_ON_L`; channel `n` is at `+ 4 * n`, 4 registers each.
pub const REG_LED0: u8 = 0x06;
pub const REG_PRESCALE: u8 = 0xFE;

/// MODE1: sleep (oscillator off), needed to set the prescaler.
pub const MODE1_SLEEP: u8 = 0x10;
/// MODE1: register auto-increment.
pub const MODE1_AUTO_INCREMENT: u8 = 0x20;
/// MODE1: restart the PWM channels after sleep.
pub const MODE1_RESTART: u8 = 0x80;
/// MODE2: open-drain outputs, not inverted (OUTDRV and INVRT clear).
pub const MODE2_OPEN_DRAIN: u8 = 0x00;

/// The board's I2C address for `bank` (1-16).
pub fn address(bank: u8) -> Option<u8> {
    (1..=BOARDS)
        .contains(&bank)
        .then(|| BASE_ADDRESS + bank - 1)
}

/// The prescaler for `frequency_hz` from the internal 25 MHz oscillator.
pub fn prescale(frequency_hz: u32) -> u8 {
    let value = (25_000_000 + 2048 * frequency_hz) / (4096 * frequency_hz);
    value.saturating_sub(1).clamp(3, 255) as u8
}

/// The 12-bit duty for a lamp value 0-255.
pub fn duty(value: u8) -> u16 {
    ((4095 * u32::from(value)) / 255) as u16
}

/// The four `LEDn_ON_L..LEDn_OFF_H` bytes for `duty` (0-4095): fully off
/// at 0, fully on at 4095, else on at 0 and off at `duty`.
pub fn channel_registers(duty: u16) -> [u8; 4] {
    match duty {
        0 => [0, 0, 0, 0x10],
        4095.. => [0, 0x10, 0, 0],
        d => {
            let off = d.to_le_bytes();
            [0, 0, off[0], off[1]]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banks_map_to_addresses() {
        assert_eq!(address(0), None);
        assert_eq!(address(1), Some(0x40));
        assert_eq!(address(16), Some(0x4F));
        assert_eq!(address(17), None);
    }

    #[test]
    fn prescale_for_1khz() {
        // round(25 MHz / (4096 * 1 kHz)) - 1
        assert_eq!(prescale(1000), 5);
        assert_eq!(prescale(200), 30);
    }

    #[test]
    fn duty_and_registers() {
        assert_eq!(duty(0), 0);
        assert_eq!(duty(255), 4095);
        assert_eq!(duty(128), 2055);
        assert_eq!(channel_registers(0), [0, 0, 0, 0x10]);
        assert_eq!(channel_registers(4095), [0, 0x10, 0, 0]);
        assert_eq!(channel_registers(2055), [0, 0, 0x07, 0x08]);
    }
}
