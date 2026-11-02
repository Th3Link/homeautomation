//! Times of day and conditions, for motion lights and schedules.

use super::{invalid, DeviceRef, Invalid};
use crate::sun::{self, Place};
use crate::tz::TimeZone;
use crate::x509::time::DAY;
use serde::{Deserialize, Serialize};

/// A time of day: on the clock, or relative to the sun.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Time {
    /// Local time.
    Clock { hour: u8, minute: u8 },
    /// A sun event, plus `offset_min` minutes (negative: before).
    Sun {
        event: sun::Event,
        #[serde(default)]
        offset_min: i16,
    },
}

impl Time {
    pub fn validate(self) -> Result<(), Invalid> {
        match self {
            Self::Clock { hour, minute } if hour > 23 || minute > 59 => {
                invalid("clock", "hour 0-23, minute 0-59")
            }
            Self::Sun { offset_min, .. } if offset_min.unsigned_abs() > 12 * 60 => {
                invalid("offset_min", "-720 to 720")
            }
            _ => Ok(()),
        }
    }
}

/// What [`Condition`]s and [`Time`]s are evaluated against.
pub struct Context<'a> {
    /// Unix seconds, `None` while the clock isn't set.
    pub now: Option<i64>,
    pub tz: &'a TimeZone,
    pub place: Option<Place>,
    /// The last brightness (lux) a sensor reported, by device key.
    pub lux: &'a dyn Fn(u32) -> Option<u32>,
}

impl Context<'_> {
    /// Local seconds now.
    pub fn local_now(&self) -> Option<i64> {
        self.now.map(|utc| self.tz.to_local(utc))
    }

    /// When `time` is on the local day `day` (days since the epoch), in
    /// local seconds. `None` for a sun time without a place, or on a day
    /// the sun doesn't reach the event.
    pub fn on_day(&self, day: i64, time: Time) -> Option<i64> {
        match time {
            Time::Clock { hour, minute } => {
                Some(day * DAY + i64::from(hour) * 3600 + i64::from(minute) * 60)
            }
            Time::Sun { event, offset_min } => {
                let utc = sun::event(day, self.place?, event)?;
                Some(self.tz.to_local(utc) + i64::from(offset_min) * 60)
            }
        }
    }
}

/// Something that holds or not at a given moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    /// A brightness sensor reads less than `lux`.
    Dark { sensor: DeviceRef, lux: u32 },
    /// A brightness sensor reads at least `lux`.
    Bright { sensor: DeviceRef, lux: u32 },
    /// Local time is from `from` to before `to` (across midnight if `to`
    /// is earlier).
    Between { from: Time, to: Time },
    /// The sun is lower than `degrees` above the horizon (negative: below
    /// it; −6 is the end of civil twilight).
    SunBelow { degrees: i8 },
    /// The sun is at least `degrees` high.
    SunAbove { degrees: i8 },
}

impl Condition {
    pub fn validate(self) -> Result<(), Invalid> {
        match self {
            Self::Between { from, to } => {
                from.validate()?;
                to.validate()
            }
            Self::SunBelow { degrees } | Self::SunAbove { degrees }
                if !(-90..=90).contains(&degrees) =>
            {
                invalid("degrees", "-90 to 90")
            }
            _ => Ok(()),
        }
    }

    /// Whether the condition holds now. One that can't be evaluated (no
    /// reading yet, no clock, no place) holds: a motion light should
    /// rather switch on than stay dark.
    pub fn holds(self, ctx: &Context<'_>) -> bool {
        self.evaluate(ctx).unwrap_or(true)
    }

    /// Whether the condition holds now, `None` if it can't be told.
    pub fn evaluate(self, ctx: &Context<'_>) -> Option<bool> {
        match self {
            Self::Dark { sensor, lux } => Some((ctx.lux)(sensor.key())? < lux),
            Self::Bright { sensor, lux } => Some((ctx.lux)(sensor.key())? >= lux),
            Self::Between { from, to } => {
                let now = ctx.local_now()?;
                let day = now.div_euclid(DAY);
                let from = ctx.on_day(day, from)?;
                let to = ctx.on_day(day, to)?;
                Some(if from <= to {
                    (from..to).contains(&now)
                } else {
                    now >= from || now < to
                })
            }
            Self::SunBelow { degrees } => {
                Some(sun::elevation(ctx.now?, ctx.place?) < f64::from(degrees))
            }
            Self::SunAbove { degrees } => {
                Some(sun::elevation(ctx.now?, ctx.place?) >= f64::from(degrees))
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::x509::time::{days_from_civil, DateTime};

    pub const BERLIN: Place = Place {
        latitude: 52.52,
        longitude: 13.405,
    };
    pub const SENSOR: DeviceRef = DeviceRef {
        device_type: 4,
        id: 77,
    };

    pub fn cet() -> TimeZone {
        TimeZone::parse(crate::tz::CENTRAL_EUROPE).unwrap()
    }

    /// Unix seconds of a local CET/CEST time.
    pub fn local(tz: &TimeZone, y: i64, mo: u32, d: u32, h: u32, mi: u32) -> i64 {
        tz.to_utc(
            DateTime {
                year: y,
                month: mo,
                day: d,
                hour: h,
                minute: mi,
                second: 0,
            }
            .to_unix()
            .unwrap(),
        )
    }

    fn no_lux(_: u32) -> Option<u32> {
        None
    }

    #[test]
    fn times_on_a_day() {
        let tz = cet();
        let ctx = Context {
            now: None,
            tz: &tz,
            place: Some(BERLIN),
            lux: &no_lux,
        };
        let day = days_from_civil(2026, 12, 21);
        let eleven = ctx
            .on_day(
                day,
                Time::Clock {
                    hour: 23,
                    minute: 0,
                },
            )
            .unwrap();
        assert_eq!(eleven - day * DAY, 23 * 3600);
        let dusk = Time::Sun {
            event: sun::Event::CivilDusk,
            offset_min: -30,
        };
        // Civil dusk 15:35:40 UTC = 16:35:40 CET, minus 30 minutes.
        let t = ctx.on_day(day, dusk).unwrap() - day * DAY;
        assert!((16 * 3600 + 5 * 60..16 * 3600 + 6 * 60).contains(&t), "{t}");
        let no_place = Context { place: None, ..ctx };
        assert_eq!(no_place.on_day(day, dusk), None);
    }

    #[test]
    fn between_across_midnight() {
        let tz = cet();
        let late = Condition::Between {
            from: Time::Clock {
                hour: 22,
                minute: 0,
            },
            to: Time::Clock { hour: 6, minute: 0 },
        };
        let at = |h| Context {
            now: Some(local(&tz, 2026, 11, 2, h, 0)),
            tz: &tz,
            place: None,
            lux: &no_lux,
        };
        assert_eq!(late.evaluate(&at(23)), Some(true));
        assert_eq!(late.evaluate(&at(5)), Some(true));
        assert_eq!(late.evaluate(&at(6)), Some(false));
        assert_eq!(late.evaluate(&at(12)), Some(false));
    }

    #[test]
    fn brightness_and_unknowns() {
        let tz = cet();
        let lux = |key| (key == SENSOR.key()).then_some(30);
        let ctx = Context {
            now: None,
            tz: &tz,
            place: None,
            lux: &lux,
        };
        assert!(Condition::Dark {
            sensor: SENSOR,
            lux: 40
        }
        .holds(&ctx));
        assert!(!Condition::Bright {
            sensor: SENSOR,
            lux: 40
        }
        .holds(&ctx));
        let other = DeviceRef {
            device_type: 4,
            id: 1,
        };
        // Never reported, no clock: can't be told, so it holds.
        assert_eq!(
            Condition::Dark {
                sensor: other,
                lux: 40
            }
            .evaluate(&ctx),
            None
        );
        assert!(Condition::Dark {
            sensor: other,
            lux: 40
        }
        .holds(&ctx));
        assert!(Condition::SunBelow { degrees: -6 }.holds(&ctx));
    }

    #[test]
    fn sun_elevation() {
        let tz = cet();
        let at = |h| Context {
            now: Some(local(&tz, 2026, 6, 21, h, 0)),
            tz: &tz,
            place: Some(BERLIN),
            lux: &no_lux,
        };
        assert_eq!(
            Condition::SunBelow { degrees: -6 }.evaluate(&at(1)),
            Some(true)
        );
        assert_eq!(
            Condition::SunBelow { degrees: -6 }.evaluate(&at(13)),
            Some(false)
        );
        assert_eq!(
            Condition::SunAbove { degrees: 30 }.evaluate(&at(13)),
            Some(true)
        );
    }

    #[test]
    fn json_shape() {
        let json = r#"{"between":{"from":{"sun":{"event":"civil_dusk","offset_min":-30}},"to":{"clock":{"hour":23,"minute":0}}}}"#;
        let (c, _): (Condition, _) = serde_json_core::from_str(json).unwrap();
        assert_eq!(
            c,
            Condition::Between {
                from: Time::Sun {
                    event: sun::Event::CivilDusk,
                    offset_min: -30
                },
                to: Time::Clock {
                    hour: 23,
                    minute: 0
                },
            }
        );
        assert!(Time::Clock {
            hour: 24,
            minute: 0
        }
        .validate()
        .is_err());
    }
}
