//! The relay boards one node drives, addressed by the `bank` of a
//! `Relais`/`Rollershutter` message, as the original C++ firmware did:
//!
//! - bank 0: the board the node sits on, its two PCA9534s at
//!   [`ONBOARD_ADDRESSES`] on the first I2C bus;
//! - banks 1-4: boards without a µC on the extension bus (their JP5/JP6
//!   bridged onto the extension connector), told apart by the A1/A2 solder
//!   bridges: bank `n` is the pair at `0x20 + 2 * (n - 1)` (A0 picks the
//!   chip within a board). The factory setting, both bridges to 3V3, is
//!   bank 4.
//!
//! A board is either the big one with 12 potential-free relays or the SSR
//! one with 16 outputs, persisted as a bitmask of SSR banks (see [`Kind`]).

/// Banks 0-4.
pub const BANKS: usize = 5;
/// The most channels a board has; the stride of [`index`].
pub const CHANNELS: usize = 16;
/// The expanders of the node's own board (bank 0).
pub const ONBOARD_ADDRESSES: [u8; 2] = [0x26, 0x27];
/// The first extension board's first expander (bank 1).
pub const EXTENSION_BASE: u8 = 0x20;

/// PCA9534 output latch.
pub const REG_OUTPUT: u8 = 0x01;
/// PCA9534 direction, a 0 bit is an output.
pub const REG_CONFIG: u8 = 0x03;

/// Which board sits at a bank.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// 12 potential-free relays (the relay module).
    Relais,
    /// 16 solid-state outputs (the SSR module).
    Ssr,
}

/// Relay module: logical relay -> (expander 0/1, output bit). P0 of both
/// expanders is unused, as is P4/P5 of the second.
const RELAIS_PINS: [(usize, u8); 12] = [
    (0, 3),
    (0, 2),
    (0, 1),
    (0, 7),
    (0, 6),
    (0, 5),
    (0, 4),
    (1, 3),
    (1, 2),
    (1, 1),
    (1, 7),
    (1, 6),
];

impl Kind {
    /// The kind of `bank` in the persisted bitmask (bit `n` set: bank `n`
    /// is an SSR board). Unset bits, and so an unset mask, mean relay
    /// modules.
    pub fn from_mask(ssr_banks: u8, bank: u8) -> Self {
        if bank < 8 && ssr_banks & (1 << bank) != 0 {
            Kind::Ssr
        } else {
            Kind::Relais
        }
    }

    pub fn channels(self) -> usize {
        match self {
            Kind::Relais => RELAIS_PINS.len(),
            Kind::Ssr => 16,
        }
    }

    /// Where channel `num` comes out: (expander 0/1, output bit).
    pub fn pin(self, num: usize) -> Option<(usize, u8)> {
        match self {
            Kind::Relais => RELAIS_PINS.get(num).copied(),
            // RELAIS1-8 on P0-P7 of the first, 9-16 of the second.
            Kind::Ssr => (num < 16).then_some((num / 8, (num % 8) as u8)),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Relais => "relais",
            Kind::Ssr => "ssr",
        }
    }
}

/// The two expanders of extension board `bank` (1-4).
pub fn extension_addresses(bank: u8) -> Option<[u8; 2]> {
    (1..BANKS as u8).contains(&bank).then(|| {
        let first = EXTENSION_BASE + 2 * (bank - 1);
        [first, first + 1]
    })
}

/// One number per channel of every bank, for schedules.
pub fn index(bank: u8, num: usize) -> usize {
    bank as usize * CHANNELS + num
}

/// The inverse of [`index`].
pub fn split(index: usize) -> (u8, usize) {
    ((index / CHANNELS) as u8, index % CHANNELS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solder_bridges_give_four_boards_clear_of_the_onboard_pair() {
        assert_eq!(extension_addresses(0), None);
        assert_eq!(extension_addresses(1), Some([0x20, 0x21]));
        assert_eq!(extension_addresses(2), Some([0x22, 0x23]));
        assert_eq!(extension_addresses(3), Some([0x24, 0x25]));
        // The factory setting.
        assert_eq!(extension_addresses(4), Some(ONBOARD_ADDRESSES));
        assert_eq!(extension_addresses(5), None);
    }

    #[test]
    fn relay_module_has_twelve_distinct_outputs() {
        let kind = Kind::Relais;
        assert_eq!(kind.channels(), 12);
        let mut seen = [[false; 8]; 2];
        for num in 0..12 {
            let (expander, bit) = kind.pin(num).unwrap();
            assert!(!seen[expander][bit as usize]);
            seen[expander][bit as usize] = true;
        }
        assert_eq!(kind.pin(12), None);
        // As the board was always driven.
        assert_eq!(kind.pin(0), Some((0, 3)));
        assert_eq!(kind.pin(11), Some((1, 6)));
    }

    #[test]
    fn ssr_module_is_linear() {
        let kind = Kind::Ssr;
        assert_eq!(kind.channels(), 16);
        assert_eq!(kind.pin(0), Some((0, 0)));
        assert_eq!(kind.pin(7), Some((0, 7)));
        assert_eq!(kind.pin(8), Some((1, 0)));
        assert_eq!(kind.pin(15), Some((1, 7)));
        assert_eq!(kind.pin(16), None);
    }

    #[test]
    fn mask_marks_ssr_banks() {
        assert_eq!(Kind::from_mask(0, 0), Kind::Relais);
        assert_eq!(Kind::from_mask(0b100, 2), Kind::Ssr);
        assert_eq!(Kind::from_mask(0b100, 1), Kind::Relais);
        assert_eq!(Kind::from_mask(0xFF, 9), Kind::Relais);
    }

    #[test]
    fn index_round_trips() {
        for bank in 0..BANKS as u8 {
            for num in 0..CHANNELS {
                assert_eq!(split(index(bank, num)), (bank, num));
            }
        }
    }
}
