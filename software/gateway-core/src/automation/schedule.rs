//! An output switched on in time windows: on at a window's start (if its
//! condition holds then), off at the first of its ends.
//!
//! Only the edges switch: switching the output off by hand in the middle of
//! a window sticks. After a restart, or when the schedule is enabled, a
//! window that is running is picked up and the output switched on.
//!
//! The outdoor light example: on 30 minutes before civil dusk, off at
//! 23:00; on again at 05:00 if it is still dark, off at sunrise + 30 min,
//! at 08:00 or when it's bright enough, whichever comes first.

use super::when::{Condition, Context, Time};
use super::{invalid, valid_name, DeviceRef, Invalid, Name};
use crate::x509::time::DAY;
use heapless::Vec;
use serde::{Deserialize, Serialize};

pub const MAX_WINDOWS: usize = 4;
pub const MAX_ENDS: usize = 3;
/// Every day of the week (bit 0 Monday … bit 6 Sunday).
pub const EVERY_DAY: u8 = 0x7F;

fn every_day() -> u8 {
    EVERY_DAY
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schedule {
    /// The output switched: a light (at its last level), dimmer or relay.
    pub output: Name,
    pub windows: Vec<Window, MAX_WINDOWS, u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Window {
    pub start: Time,
    /// Checked once, at the start: the window only runs if it holds (one
    /// that can't be told holds).
    #[serde(default)]
    pub only_if: Option<Condition>,
    /// The window ends at the first of these.
    pub end: Vec<End, MAX_ENDS, u8>,
    /// The days it starts on: bit 0 Monday … bit 6 Sunday.
    #[serde(default = "every_day")]
    pub days: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum End {
    /// At a time (the next one after the start).
    At(Time),
    /// When a brightness sensor has read at least `lux` for `for_min`
    /// minutes in a row.
    Bright {
        sensor: DeviceRef,
        lux: u32,
        #[serde(default)]
        for_min: u16,
    },
}

impl Schedule {
    pub fn validate(&self) -> Result<(), Invalid> {
        if !valid_name(&self.output) {
            return invalid("output", "an output name");
        }
        if self.windows.is_empty() {
            return invalid("windows", "at least one");
        }
        for w in &self.windows {
            w.start.validate()?;
            if let Some(c) = w.only_if {
                c.validate()?;
            }
            if !w.end.iter().any(|e| matches!(e, End::At(_))) {
                return invalid("windows.end", "at least one time");
            }
            for e in &w.end {
                if let End::At(t) = e {
                    t.validate()?;
                }
            }
            if w.days == 0 || w.days > EVERY_DAY {
                return invalid(
                    "windows.days",
                    "bits 0 (Monday) to 6 (Sunday), at least one",
                );
            }
        }
        Ok(())
    }
}

/// What the schedule wants done.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Do {
    On,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Running {
    /// The first time end, local seconds.
    until: i64,
    /// Since when a `Bright` end's sensor has been bright, local seconds;
    /// 0: not bright.
    bright_since: i64,
}

/// A schedule's state. Times are local seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct State {
    pub enabled: bool,
    /// When it was last evaluated; `None` after a start or enabling, so
    /// the next evaluation picks up a running window.
    last: Option<i64>,
    /// Per window; `until == 0`: not running.
    running: [Running; MAX_WINDOWS],
}

/// Monday 0 … Sunday 6 of a local day.
fn weekday(day: i64) -> u32 {
    // 1970-01-01 was a Thursday.
    u32::try_from((day + 3).rem_euclid(7)).unwrap_or(0)
}

/// The first time end of `window` after `start` (on the local day `day`).
fn time_end(window: &Window, ctx: &Context<'_>, day: i64, start: i64) -> Option<i64> {
    window
        .end
        .iter()
        .filter_map(|e| match e {
            End::At(t) => {
                let at = ctx.on_day(day, *t)?;
                if at > start {
                    Some(at)
                } else {
                    ctx.on_day(day + 1, *t)
                }
            }
            End::Bright { .. } => None,
        })
        .min()
}

impl State {
    pub const fn new(enabled: bool) -> Self {
        Self {
            enabled,
            last: None,
            running: [Running {
                until: 0,
                bright_since: 0,
            }; MAX_WINDOWS],
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        *self = Self::new(enabled);
    }

    /// Whether any window is running.
    pub fn running(&self) -> bool {
        self.running.iter().any(|r| r.until != 0)
    }

    /// Checks the windows at the context's time. Returns what to switch,
    /// in order (an end before a start at the same moment). Nothing while
    /// the clock isn't set.
    pub fn evaluate(
        &mut self,
        config: &Schedule,
        ctx: &Context<'_>,
    ) -> Vec<Do, { 2 * MAX_WINDOWS }, u8> {
        let mut out = Vec::new();
        let Some(now) = ctx.local_now() else {
            return out;
        };
        if !self.enabled {
            return out;
        }
        let today = now.div_euclid(DAY);
        for (window, running) in config.windows.iter().zip(self.running.iter_mut()) {
            if running.until != 0 {
                let r = &mut *running;
                let mut ended = now >= r.until;
                for end in &window.end {
                    if let End::Bright {
                        sensor,
                        lux,
                        for_min,
                    } = *end
                    {
                        if (ctx.lux)(sensor.key()).is_some_and(|l| l >= lux) {
                            if r.bright_since == 0 {
                                r.bright_since = now;
                            }
                            ended |= now - r.bright_since >= i64::from(for_min) * 60;
                        } else {
                            r.bright_since = 0;
                        }
                    }
                }
                if ended {
                    *running = Running::default();
                    let _ = out.push(Do::Off);
                }
                continue;
            }
            for day in [today - 1, today] {
                if window.days & (1 << weekday(day)) == 0 {
                    continue;
                }
                let Some(start) = ctx.on_day(day, window.start) else {
                    continue;
                };
                let Some(until) = time_end(window, ctx, day, start) else {
                    continue;
                };
                let starts = match self.last {
                    Some(last) => last < start && start <= now,
                    // Picked up: running now.
                    None => start <= now,
                };
                if !starts || now >= until {
                    continue;
                }
                if window.only_if.is_some_and(|c| !c.holds(ctx)) {
                    continue;
                }
                *running = Running {
                    until,
                    bright_since: 0,
                };
                let _ = out.push(Do::On);
                break;
            }
        }
        self.last = Some(now);
        // Ends first, then starts.
        out.sort_unstable_by_key(|d| *d == Do::On);
        out
    }

    /// The next local time [`State::evaluate`] has something to do: a
    /// start or a time end (brightness ends need the sensor's reports).
    pub fn deadline(&self, config: &Schedule, ctx: &Context<'_>) -> Option<i64> {
        let now = ctx.local_now()?;
        if !self.enabled {
            return None;
        }
        let today = now.div_euclid(DAY);
        let mut next: Option<i64> = None;
        let mut consider = |t: i64| {
            if t > now {
                next = Some(next.map_or(t, |n| n.min(t)));
            }
        };
        for (window, running) in config.windows.iter().zip(&self.running) {
            match (running.until != 0).then_some(running) {
                Some(r) => {
                    consider(r.until);
                    for end in &window.end {
                        if let End::Bright { for_min, .. } = end {
                            if r.bright_since != 0 {
                                consider(r.bright_since + i64::from(*for_min) * 60);
                            }
                        }
                    }
                }
                None => {
                    for day in [today, today + 1] {
                        if window.days & (1 << weekday(day)) != 0 {
                            if let Some(start) = ctx.on_day(day, window.start) {
                                consider(start);
                            }
                        }
                    }
                }
            }
        }
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::when::tests::{cet, local, BERLIN, SENSOR};
    use crate::tz::TimeZone;
    use crate::x509::time::{days_from_civil, DateTime};

    fn outdoor() -> Schedule {
        let json = r#"{"output":"aussen","windows":[
            {"start":{"sun":{"event":"civil_dusk","offset_min":-30}},
             "end":[{"at":{"clock":{"hour":23,"minute":0}}}]},
            {"start":{"clock":{"hour":5,"minute":0}},
             "only_if":{"sun_below":{"degrees":-6}},
             "end":[{"at":{"sun":{"event":"sunrise","offset_min":30}}},
                    {"at":{"clock":{"hour":8,"minute":0}}},
                    {"bright":{"sensor":{"type":4,"id":77},"lux":200,"for_min":5}}]}]}"#;
        let (s, _): (Schedule, _) = serde_json_core::from_str(json).unwrap();
        s.validate().unwrap();
        s
    }

    fn ctx_at<'a>(tz: &'a TimeZone, utc: i64, lux: &'a dyn Fn(u32) -> Option<u32>) -> Context<'a> {
        Context {
            now: Some(utc),
            tz,
            place: Some(BERLIN),
            lux,
        }
    }

    fn dark(_: u32) -> Option<u32> {
        Some(0)
    }

    /// Steps through `from`..`to` (UTC) a minute at a time and records
    /// the switches with their local `HH:MM`.
    fn run(
        s: &Schedule,
        state: &mut State,
        tz: &TimeZone,
        from: i64,
        to: i64,
        lux: &dyn Fn(u32) -> Option<u32>,
    ) -> std::vec::Vec<(Do, (u32, u32))> {
        let mut log = std::vec::Vec::new();
        let mut t = from;
        while t < to {
            for d in state.evaluate(s, &ctx_at(tz, t, lux)) {
                let local = DateTime::from_unix(tz.to_local(t));
                log.push((d, (local.hour, local.minute)));
            }
            t += 60;
        }
        log
    }

    #[test]
    fn outdoor_light_on_a_winter_day() {
        let tz = cet();
        let s = outdoor();
        let mut state = State::new(true);
        // From noon on 21 December to noon the next day, always dark.
        let from = local(&tz, 2026, 12, 21, 12, 0);
        let log = run(&s, &mut state, &tz, from, from + DAY, &dark);
        // Civil dusk 16:35 → 16:05; sunrise 08:15 + 30 is after 08:00.
        assert_eq!(
            log,
            [
                (Do::On, (16, 6)),
                (Do::Off, (23, 0)),
                (Do::On, (5, 0)),
                (Do::Off, (8, 0))
            ]
        );
    }

    #[test]
    fn outdoor_light_in_summer_skips_the_morning() {
        let tz = cet();
        let s = outdoor();
        let mut state = State::new(true);
        let from = local(&tz, 2026, 6, 21, 12, 0);
        let log = run(&s, &mut state, &tz, from, from + DAY, &dark);
        // Civil dusk 22:23 CEST → 21:53; at 05:00 the sun is up.
        assert_eq!(log, [(Do::On, (21, 54)), (Do::Off, (23, 0))]);
    }

    #[test]
    fn brightness_ends_the_morning_window_early() {
        let tz = cet();
        let s = outdoor();
        let mut state = State::new(true);
        let bright_from = local(&tz, 2026, 12, 22, 7, 30);
        let from = local(&tz, 2026, 12, 22, 4, 0);
        let mut log = run(&s, &mut state, &tz, from, bright_from, &dark);
        let bright = |key: u32| (key == SENSOR.key()).then_some(500);
        log.extend(run(
            &s,
            &mut state,
            &tz,
            bright_from,
            bright_from + 3600,
            &bright,
        ));
        // Bright for 5 minutes: off at 07:35.
        assert_eq!(log, [(Do::On, (5, 0)), (Do::Off, (7, 35))]);
    }

    #[test]
    fn a_restart_inside_a_window_picks_it_up() {
        let tz = cet();
        let s = outdoor();
        let mut state = State::new(true);
        let at = local(&tz, 2026, 12, 21, 20, 0);
        assert_eq!(
            state.evaluate(&s, &ctx_at(&tz, at, &dark)).as_slice(),
            [Do::On]
        );
        // Already running: nothing more.
        assert!(state.evaluate(&s, &ctx_at(&tz, at + 60, &dark)).is_empty());
        // Picked up after midnight too (the window from the day before).
        let mut state = State::new(true);
        let night = local(&tz, 2026, 12, 21, 22, 59);
        assert_eq!(
            state.evaluate(&s, &ctx_at(&tz, night, &dark)).as_slice(),
            [Do::On]
        );
        // Outside every window: nothing.
        let mut state = State::new(true);
        let day = local(&tz, 2026, 12, 21, 12, 0);
        assert!(state.evaluate(&s, &ctx_at(&tz, day, &dark)).is_empty());
    }

    #[test]
    fn deadlines() {
        let tz = cet();
        let s = outdoor();
        let mut state = State::new(true);
        let noon = local(&tz, 2026, 12, 21, 12, 0);
        state.evaluate(&s, &ctx_at(&tz, noon, &dark));
        let next = state.deadline(&s, &ctx_at(&tz, noon, &dark)).unwrap();
        let t = DateTime::from_unix(next);
        assert_eq!((t.hour, t.minute), (16, 5));
        let evening = local(&tz, 2026, 12, 21, 17, 0);
        state.evaluate(&s, &ctx_at(&tz, evening, &dark));
        let next = state.deadline(&s, &ctx_at(&tz, evening, &dark)).unwrap();
        assert_eq!(next, tz.to_local(local(&tz, 2026, 12, 21, 23, 0)));
    }

    #[test]
    fn disabled_and_unknown_time_do_nothing() {
        let tz = cet();
        let s = outdoor();
        let mut state = State::new(false);
        let at = local(&tz, 2026, 12, 21, 20, 0);
        assert!(state.evaluate(&s, &ctx_at(&tz, at, &dark)).is_empty());
        state.set_enabled(true);
        assert_eq!(
            state.evaluate(&s, &ctx_at(&tz, at, &dark)).as_slice(),
            [Do::On]
        );
        let mut state = State::new(true);
        let no_clock = Context {
            now: None,
            tz: &tz,
            place: Some(BERLIN),
            lux: &dark,
        };
        assert!(state.evaluate(&s, &no_clock).is_empty());
    }

    #[test]
    fn weekdays() {
        // 2 November 2026 is a Monday.
        assert_eq!(weekday(days_from_civil(2026, 11, 2)), 0);
        assert_eq!(weekday(days_from_civil(2026, 11, 8)), 6);
        let tz = cet();
        let mut s = outdoor();
        // Weekends only.
        s.windows[0].days = 0b110_0000;
        let mut state = State::new(true);
        let monday = local(&tz, 2026, 11, 2, 20, 0);
        assert!(state.evaluate(&s, &ctx_at(&tz, monday, &dark)).is_empty());
        let mut state = State::new(true);
        let saturday = local(&tz, 2026, 11, 7, 20, 0);
        assert_eq!(
            state.evaluate(&s, &ctx_at(&tz, saturday, &dark)).as_slice(),
            [Do::On]
        );
    }

    #[test]
    fn validation() {
        let mut s = outdoor();
        s.windows[0].end.clear();
        assert_eq!(s.validate().unwrap_err().field, "windows.end");
    }
}
