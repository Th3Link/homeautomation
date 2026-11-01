//! The VEML7700 ambient light sensor: its settings, lux from its counts,
//! and when a reading is worth sending (`AmbientLightSensor`, u32 lux LE).
//!
//! One setting can't cover a dark hallway and a sunlit room, so there are
//! two ranges, switched with hysteresis:
//!
//! | Range | Gain | Integration | lx per count | Up to |
//! |---|---|---|---|---|
//! | [`Range::Dim`] | ×2 | 400 ms | 0.0084 | ~550 lx |
//! | [`Range::Bright`] | ×1/8 | 100 ms | 0.5376 | ~35 klx |
//!
//! The resolutions are Vishay's (application note "Designing the VEML7700
//! Into an Application", 0.0042 lx/count at ×2 and 800 ms). Above 1000 lx
//! the bright range is corrected for the sensor's non-linearity with the
//! application note's polynomial.

/// I2C address.
pub const ADDRESS: u8 = 0x10;
/// Register: configuration (`ALS_CONF_0`).
pub const REG_CONF: u8 = 0x00;
/// Register: power saving (off: written 0).
pub const REG_POWER_SAVING: u8 = 0x03;
/// Register: the ambient light count.
pub const REG_ALS: u8 = 0x04;

/// A measuring range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Range {
    Dim,
    Bright,
}

/// In [`Range::Dim`], counts above this switch to [`Range::Bright`].
const DIM_MAX: u16 = 50_000;
/// In [`Range::Bright`], counts below this (~270 lx) switch to [`Range::Dim`].
const BRIGHT_MIN: u16 = 500;

impl Range {
    /// The `ALS_CONF_0` value: gain, integration time, powered on, no
    /// interrupt.
    pub fn config(self) -> u16 {
        // (gain, integration time) codes of the datasheet.
        let (gain, it) = match self {
            Range::Dim => (0b01, 0b0010),
            Range::Bright => (0b10, 0b0000),
        };
        (gain << 11) | (it << 6)
    }

    /// How long a measurement takes; after a change of range, wait twice
    /// this before reading.
    pub fn integration_ms(self) -> u32 {
        match self {
            Range::Dim => 400,
            Range::Bright => 100,
        }
    }

    /// Lux for `counts` measured in this range.
    pub fn lux(self, counts: u16) -> u32 {
        let counts = f32::from(counts);
        let lux = match self {
            Range::Dim => counts * 0.0084,
            Range::Bright => {
                let lux = counts * 0.5376;
                if lux > 1000.0 {
                    // Vishay's correction, for gain ×1/4 and ×1/8.
                    let x = lux;
                    6.0135e-13 * x * x * x * x - 9.3924e-9 * x * x * x
                        + 8.1488e-5 * x * x
                        + 1.0023 * x
                } else {
                    lux
                }
            }
        };
        // Rounded; never negative.
        (lux + 0.5) as u32
    }

    /// The range for the next measurement, after one with `counts`.
    pub fn next(self, counts: u16) -> Range {
        match self {
            Range::Dim if counts > DIM_MAX => Range::Bright,
            Range::Bright if counts < BRIGHT_MIN => Range::Dim,
            range => range,
        }
    }
}

/// Decides which readings are sent: a change of more than 10 % (at least
/// 2 lx), else one every `interval_s`, so a light switching on is seen at
/// once without a reading every few seconds when nothing changes.
#[derive(Default)]
pub struct Reporter {
    last: Option<(u32, u64)>,
}

impl Reporter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `lux`, measured at `now_s`, is to be sent; if so, it is
    /// taken as sent.
    pub fn report(&mut self, lux: u32, now_s: u64, interval_s: u64) -> bool {
        let send = match self.last {
            None => true,
            Some((sent, at)) => {
                let threshold = (sent / 10).max(2);
                lux.abs_diff(sent) > threshold || now_s.saturating_sub(at) >= interval_s
            }
        };
        if send {
            self.last = Some((lux, now_s));
        }
        send
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_words() {
        // Gain ×2 (01), 400 ms (0010).
        assert_eq!(Range::Dim.config(), 0b01 << 11 | 0b0010 << 6);
        // Gain ×1/8 (10), 100 ms (0000).
        assert_eq!(Range::Bright.config(), 0b10 << 11);
    }

    #[test]
    fn lux_per_range() {
        assert_eq!(Range::Dim.lux(0), 0);
        assert_eq!(Range::Dim.lux(10_000), 84);
        assert_eq!(Range::Bright.lux(1_000), 538);
        // Corrected above 1000 lx: a bit more than linear.
        assert!(Range::Bright.lux(20_000) > 10_752);
    }

    #[test]
    fn range_switches_with_hysteresis() {
        assert_eq!(Range::Dim.next(40_000), Range::Dim);
        assert_eq!(Range::Dim.next(60_000), Range::Bright);
        assert_eq!(Range::Bright.next(1_000), Range::Bright);
        assert_eq!(Range::Bright.next(100), Range::Dim);
        // The ranges overlap: the switch points don't flip back.
        assert!(Range::Bright.lux(BRIGHT_MIN) < Range::Dim.lux(DIM_MAX));
    }

    #[test]
    fn reporter_sends_changes_and_a_heartbeat() {
        let mut r = Reporter::new();
        assert!(r.report(100, 0, 30));
        assert!(!r.report(105, 5, 30));
        assert!(r.report(120, 10, 30));
        assert!(!r.report(121, 20, 30));
        assert!(r.report(121, 40, 30));
        // Small values: at least 2 lx.
        let mut r = Reporter::new();
        assert!(r.report(0, 0, 30));
        assert!(!r.report(2, 1, 30));
        assert!(r.report(3, 2, 30));
    }
}
