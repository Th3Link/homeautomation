//! A light switched by motion detectors.
//!
//! Motion switches the light on (if the conditions hold) and it goes off
//! again `timeout_s` after the last motion. Switching it on by hand (a
//! button, Home Assistant) stops the timer until it is switched off. After
//! any off, motion is ignored for `lockout_s`, so the light doesn't come
//! straight back on for someone leaving the room. Disabled, it ignores
//! motion and leaves the light alone.
//!
//! The level it switches on at can depend on the time of day: from each
//! [`Period`]'s start on, a fixed level (at night: 1) or the last level
//! with a minimum (in the daytime: at least 4).

use super::binding::Op;
use super::when::{Condition, Context, Time};
use super::{invalid, valid_name, DeviceRef, Invalid, Name};
use crate::x509::time::DAY;
use heapless::Vec;
use serde::{Deserialize, Serialize};

/// The most motion detectors one motion light listens to.
pub const MAX_DETECTORS: usize = 4;
/// The most conditions of one motion light.
pub const MAX_CONDITIONS: usize = 3;
/// The most periods with their own level.
pub const MAX_PERIODS: usize = 4;

/// From `from` on (every day, until the next period starts), switch on at
/// `level`; without one, at the last level but at least `min`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Period {
    pub from: Time,
    #[serde(default)]
    pub level: Option<u8>,
    #[serde(default)]
    pub min: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MotionLight {
    /// Devices whose motion detector (`PirSensor`) switches the light.
    pub detectors: Vec<DeviceRef, MAX_DETECTORS, u8>,
    /// The output switched: a light, dimmer or relay.
    pub output: Name,
    /// The level (light) or value (dimmer) to switch on at, by the time of
    /// day; without any, the last one.
    #[serde(default)]
    pub levels: Vec<Period, MAX_PERIODS, u8>,
    pub timeout_s: u32,
    pub lockout_s: u32,
    /// All must hold for motion to switch the light on.
    #[serde(default)]
    pub conditions: Vec<Condition, MAX_CONDITIONS, u8>,
}

impl MotionLight {
    pub fn validate(&self) -> Result<(), Invalid> {
        if self.detectors.is_empty() {
            return invalid("detectors", "at least one");
        }
        if !valid_name(&self.output) {
            return invalid("output", "an output name");
        }
        if self.timeout_s == 0 || self.timeout_s > 24 * 3600 {
            return invalid("timeout_s", "1-86400");
        }
        if self.lockout_s > 3600 {
            return invalid("lockout_s", "0-3600");
        }
        for p in &self.levels {
            p.from.validate()?;
            if p.level == Some(0) {
                return invalid("levels.level", "1-255, or none for the last level");
            }
        }
        self.conditions.iter().try_for_each(|c| c.validate())
    }

    /// What switches the light on now: the level of the period that
    /// started last (today, or yesterday's last one), else the last level.
    pub fn on_op(&self, ctx: &Context<'_>) -> Op {
        let Some(now) = ctx.local_now() else {
            return Op::On;
        };
        let day = now.div_euclid(DAY);
        let current = self
            .levels
            .iter()
            .flat_map(|p| [day - 1, day].map(|d| (ctx.on_day(d, p.from), p)))
            .filter_map(|(at, p)| Some((at?, p)))
            .filter(|(at, _)| *at <= now)
            .max_by_key(|(at, _)| *at);
        match current {
            Some((_, Period { level: Some(n), .. })) => Op::Set(*n),
            Some((_, Period { min, .. })) if *min > 0 => Op::AtLeast(*min),
            _ => Op::On,
        }
    }
}

/// Where a motion light is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Waiting for motion (light off, or on but not by this).
    Idle,
    /// Switched on by motion; off at `off_at` (ms).
    Auto { off_at: u64 },
    /// Switched on by hand; stays on.
    Manual,
    /// Switched off; motion is ignored until `until` (ms).
    Lockout { until: u64 },
}

/// What the motion light wants done.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Do {
    On,
    Off,
}

/// A motion light's state. Times are monotonic milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct State {
    pub mode: Mode,
    pub enabled: bool,
}

impl State {
    pub const fn new(enabled: bool) -> Self {
        Self {
            mode: Mode::Idle,
            enabled,
        }
    }

    /// A detector saw motion. `conditions` tells whether the conditions
    /// hold (only asked when it matters).
    pub fn motion(
        &mut self,
        config: &MotionLight,
        now: u64,
        conditions: impl FnOnce() -> bool,
    ) -> Option<Do> {
        if !self.enabled {
            return None;
        }
        let off_at = now + u64::from(config.timeout_s) * 1000;
        match self.mode {
            Mode::Idle => {
                if !conditions() {
                    return None;
                }
                self.mode = Mode::Auto { off_at };
                Some(Do::On)
            }
            Mode::Auto { .. } => {
                self.mode = Mode::Auto { off_at };
                None
            }
            Mode::Manual | Mode::Lockout { .. } => None,
        }
    }

    /// Something other than this motion light switched the light on.
    pub fn switched_on(&mut self) {
        if self.enabled {
            self.mode = Mode::Manual;
        }
    }

    /// Something other than this motion light switched the light off.
    pub fn switched_off(&mut self, config: &MotionLight, now: u64) {
        if self.enabled {
            self.mode = Mode::Lockout {
                until: now + u64::from(config.lockout_s) * 1000,
            };
        }
    }

    /// Time passes: the timeout and the lockout end.
    pub fn tick(&mut self, config: &MotionLight, now: u64) -> Option<Do> {
        match self.mode {
            Mode::Auto { off_at } if now >= off_at => {
                self.mode = Mode::Lockout {
                    until: now + u64::from(config.lockout_s) * 1000,
                };
                Some(Do::Off)
            }
            Mode::Lockout { until } if now >= until => {
                self.mode = Mode::Idle;
                None
            }
            _ => None,
        }
    }

    /// When [`State::tick`] has something to do next.
    pub fn deadline(&self) -> Option<u64> {
        match self.mode {
            Mode::Auto { off_at } => Some(off_at),
            Mode::Lockout { until } => Some(until),
            Mode::Idle | Mode::Manual => None,
        }
    }

    /// Enables or disables it; either way it starts over, and a light it
    /// switched on stays on (without a timer).
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.mode = Mode::Idle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> MotionLight {
        MotionLight {
            detectors: Vec::from_slice(&[DeviceRef {
                device_type: 4,
                id: 77,
            }])
            .unwrap(),
            output: Name::try_from("bad").unwrap(),
            levels: Vec::new(),
            timeout_s: 180,
            lockout_s: 60,
            conditions: Vec::new(),
        }
    }

    const S: u64 = 1000;

    #[test]
    fn motion_on_then_off_after_the_timeout_then_locked() {
        let c = config();
        let mut s = State::new(true);
        assert_eq!(s.motion(&c, 0, || true), Some(Do::On));
        // More motion pushes the timeout.
        assert_eq!(s.motion(&c, 100 * S, || true), None);
        assert_eq!(s.deadline(), Some(280 * S));
        assert_eq!(s.tick(&c, 279 * S), None);
        assert_eq!(s.tick(&c, 280 * S), Some(Do::Off));
        // Locked for a minute.
        assert_eq!(s.motion(&c, 300 * S, || true), None);
        assert_eq!(s.tick(&c, 340 * S), None);
        assert_eq!(s.mode, Mode::Idle);
        assert_eq!(s.motion(&c, 341 * S, || true), Some(Do::On));
    }

    #[test]
    fn conditions_only_matter_for_switching_on() {
        let c = config();
        let mut s = State::new(true);
        assert_eq!(s.motion(&c, 0, || false), None);
        assert_eq!(s.mode, Mode::Idle);
        s.motion(&c, 1, || true);
        // Bright now (the light is on): still keeps it on.
        let mut asked = false;
        s.motion(&c, 2, || {
            asked = true;
            false
        });
        assert!(!asked);
        assert!(matches!(s.mode, Mode::Auto { .. }));
    }

    #[test]
    fn switched_on_by_hand_has_no_timer_until_off() {
        let c = config();
        let mut s = State::new(true);
        s.motion(&c, 0, || true);
        s.switched_on();
        assert_eq!(s.mode, Mode::Manual);
        assert_eq!(s.deadline(), None);
        assert_eq!(s.tick(&c, 1000 * S), None);
        assert_eq!(s.motion(&c, 1000 * S, || true), None);
        s.switched_off(&c, 1000 * S);
        assert_eq!(s.motion(&c, 1030 * S, || true), None);
        s.tick(&c, 1060 * S);
        assert_eq!(s.motion(&c, 1061 * S, || true), Some(Do::On));
    }

    #[test]
    fn switched_off_by_hand_while_auto_locks() {
        let c = config();
        let mut s = State::new(true);
        s.motion(&c, 0, || true);
        s.switched_off(&c, 10 * S);
        assert_eq!(s.mode, Mode::Lockout { until: 70 * S });
    }

    #[test]
    fn disabled_ignores_everything() {
        let c = config();
        let mut s = State::new(true);
        s.motion(&c, 0, || true);
        s.set_enabled(false);
        assert_eq!(s.tick(&c, 1000 * S), None);
        assert_eq!(s.motion(&c, 1000 * S, || true), None);
        s.switched_on();
        assert_eq!(s.mode, Mode::Idle);
        s.set_enabled(true);
        assert_eq!(s.motion(&c, 1001 * S, || true), Some(Do::On));
    }

    #[test]
    fn json_and_validation() {
        let json = r#"{"detectors":[{"type":4,"id":77}],"output":"bad","timeout_s":180,"lockout_s":60,
            "conditions":[{"dark":{"sensor":{"type":4,"id":77},"lux":40}}]}"#;
        let (m, _): (MotionLight, _) = serde_json_core::from_str(json).unwrap();
        assert_eq!(m.validate(), Ok(()));
        assert!(m.levels.is_empty());
        let bad = MotionLight { timeout_s: 0, ..m };
        assert_eq!(bad.validate().unwrap_err().field, "timeout_s");
    }

    #[test]
    fn level_by_the_time_of_day() {
        use crate::automation::when::tests::{cet, local, BERLIN};
        // At night (from 22:00) level 1; in the daytime (from sunrise) the
        // last level, but at least 4.
        let json = r#"{"detectors":[{"type":4,"id":77}],"output":"flur","timeout_s":180,"lockout_s":60,
            "levels":[{"from":{"clock":{"hour":22,"minute":0}},"level":1},
                      {"from":{"sun":{"event":"sunrise","offset_min":0}},"min":4}]}"#;
        let (m, _): (MotionLight, _) = serde_json_core::from_str(json).unwrap();
        assert_eq!(m.validate(), Ok(()));
        let tz = cet();
        let no_lux = |_| None;
        let at = |h, mi| {
            let ctx = Context {
                now: Some(local(&tz, 2026, 6, 21, h, mi)),
                tz: &tz,
                place: Some(BERLIN),
                lux: &no_lux,
            };
            m.on_op(&ctx)
        };
        assert_eq!(at(23, 0), Op::Set(1));
        // After midnight: still yesterday's 22:00 period.
        assert_eq!(at(2, 0), Op::Set(1));
        assert_eq!(at(12, 0), Op::AtLeast(4));
        // Without a clock: the last level.
        let ctx = Context {
            now: None,
            tz: &tz,
            place: Some(BERLIN),
            lux: &no_lux,
        };
        assert_eq!(m.on_op(&ctx), Op::On);
        let mut zero = m.clone();
        zero.levels[0].level = Some(0);
        assert_eq!(zero.validate().unwrap_err().field, "levels.level");
    }
}
