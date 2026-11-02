//! Local time from a POSIX `TZ` string, e.g. `CET-1CEST,M3.5.0,M10.5.0/3`
//! for Central Europe.
//!
//! A `TZ` string holds a zone's standard offset and, optionally, its
//! daylight saving offset and the two rules for when it starts and ends.
//! That is all a gateway needs to switch at local times, without a time
//! zone database. It doesn't know historical rule changes; the rules apply
//! to every year.
//!
//! Format (POSIX, plus the common extension of hours beyond 24 and
//! negative in the rule times): `std offset [dst [offset] [,start[/time],end[/time]]]`.
//! - Names are 3+ letters, or anything in `<…>` (e.g. `<+03>`).
//! - Offsets are `[+-]hh[:mm[:ss]]` **west** of UTC: CET (UTC+1) is `-1`.
//!   Without its own offset, daylight time is one hour ahead of standard.
//! - Rules are `Mm.w.d` (day `d`, 0 = Sunday, of week `w` of month `m`; week
//!   5 is the last), `Jn` (day 1–365, never counting 29 February) or `n`
//!   (day 0–365). The time defaults to 02:00 local.
//! - Without rules, daylight time follows the US rules (`M3.2.0,M11.1.0`).

use crate::x509::time::{civil_from_days, days_from_civil, DAY};

/// Central European Time, the default.
pub const CENTRAL_EUROPE: &str = "CET-1CEST,M3.5.0,M10.5.0/3";

/// The longest `TZ` string accepted.
pub const TZ_CAP: usize = 64;

/// What was wrong with a `TZ` string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TzError {
    /// Longer than [`TZ_CAP`].
    TooLong,
    /// A zone name is missing or shorter than 3 letters.
    Name,
    /// An offset or a rule time is missing or out of range.
    Offset,
    /// A start or end rule is malformed.
    Rule,
    /// Characters after the end.
    Trailing,
}

/// A day in the year on which daylight time starts or ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Day {
    /// `Mm.w.d`.
    Weekday { month: u8, week: u8, weekday: u8 },
    /// `Jn`: 1–365, 29 February not counted.
    Julian1(u16),
    /// `n`: 0–365, 29 February counted.
    Julian0(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rule {
    day: Day,
    /// Seconds after local midnight, may be negative or beyond a day.
    time: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Daylight {
    /// Seconds east of UTC.
    offset: i32,
    start: Rule,
    end: Rule,
}

/// A parsed `TZ` string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeZone {
    /// Seconds east of UTC.
    standard: i32,
    daylight: Option<Daylight>,
}

impl TimeZone {
    /// UTC, no daylight saving.
    pub const UTC: Self = Self {
        standard: 0,
        daylight: None,
    };

    pub fn parse(tz: &str) -> Result<Self, TzError> {
        if tz.len() > TZ_CAP {
            return Err(TzError::TooLong);
        }
        let mut p = Parser(tz.as_bytes());
        p.name()?;
        let standard = -p.offset()?;
        if p.0.is_empty() {
            return Ok(Self {
                standard,
                daylight: None,
            });
        }
        p.name()?;
        let offset = if p.0.first().is_some_and(|c| *c != b',') {
            -p.offset()?
        } else {
            standard + 3600
        };
        let (start, end) = if p.eat(b',') {
            let start = p.rule()?;
            if !p.eat(b',') {
                return Err(TzError::Rule);
            }
            (start, p.rule()?)
        } else {
            (US_START, US_END)
        };
        if !p.0.is_empty() {
            return Err(TzError::Trailing);
        }
        Ok(Self {
            standard,
            daylight: Some(Daylight { offset, start, end }),
        })
    }

    /// The offset east of UTC, in seconds, at the instant `utc` (Unix
    /// seconds).
    pub fn offset_at(&self, utc: i64) -> i32 {
        let Some(dst) = self.daylight else {
            return self.standard;
        };
        let (year, _, _) = civil_from_days((utc + i64::from(self.standard)).div_euclid(DAY));
        // The start is given in standard time, the end in daylight time.
        let start = transition(year, dst.start) - i64::from(self.standard);
        let end = transition(year, dst.end) - i64::from(dst.offset);
        let is_dst = if start < end {
            (start..end).contains(&utc)
        } else {
            // Southern hemisphere: daylight time spans New Year.
            !(end..start).contains(&utc)
        };
        if is_dst {
            dst.offset
        } else {
            self.standard
        }
    }

    /// Local seconds (Unix seconds shifted by the offset) at `utc`.
    pub fn to_local(&self, utc: i64) -> i64 {
        utc + i64::from(self.offset_at(utc))
    }

    /// The instant a local time stands for. In the hour skipped in spring
    /// it is the instant one hour later (02:30 → 03:30 CEST); in the hour
    /// repeated in autumn it is the first one (still daylight time).
    pub fn to_utc(&self, local: i64) -> i64 {
        let Some(dst) = self.daylight else {
            return local - i64::from(self.standard);
        };
        let as_dst = local - i64::from(dst.offset);
        let as_standard = local - i64::from(self.standard);
        let dst_fits = self.offset_at(as_dst) == dst.offset;
        let standard_fits = self.offset_at(as_standard) == self.standard;
        match (dst_fits, standard_fits) {
            (true, true) => as_dst.min(as_standard),
            (true, false) => as_dst,
            // The skipped hour: read it with the offset before the change.
            (false, true) | (false, false) => as_standard,
        }
    }
}

const US_START: Rule = Rule {
    day: Day::Weekday {
        month: 3,
        week: 2,
        weekday: 0,
    },
    time: 2 * 3600,
};
const US_END: Rule = Rule {
    day: Day::Weekday {
        month: 11,
        week: 1,
        weekday: 0,
    },
    time: 2 * 3600,
};

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// The local seconds (Unix seconds in local time) of `rule` in `year`.
fn transition(year: i64, rule: Rule) -> i64 {
    let day = match rule.day {
        Day::Weekday {
            month,
            week,
            weekday,
        } => {
            let month = u32::from(month);
            let first = days_from_civil(year, month, 1);
            // 1970-01-01 was a Thursday (4).
            let first_weekday = (first + 4).rem_euclid(7);
            let mut day = first + (i64::from(weekday) - first_weekday).rem_euclid(7);
            day += 7 * (i64::from(week) - 1);
            let last = first + i64::from(days_in_month(year, month)) - 1;
            while day > last {
                day -= 7;
            }
            day
        }
        Day::Julian1(n) => {
            let n = i64::from(n);
            let jan1 = days_from_civil(year, 1, 1);
            // Days from 1 March on skip 29 February.
            if is_leap(year) && n >= 60 {
                jan1 + n
            } else {
                jan1 + n - 1
            }
        }
        Day::Julian0(n) => days_from_civil(year, 1, 1) + i64::from(n),
    };
    day * DAY + i64::from(rule.time)
}

struct Parser<'a>(&'a [u8]);

impl Parser<'_> {
    fn eat(&mut self, c: u8) -> bool {
        if self.0.first() == Some(&c) {
            self.0 = &self.0[1..];
            true
        } else {
            false
        }
    }

    fn name(&mut self) -> Result<(), TzError> {
        if self.eat(b'<') {
            let end = self
                .0
                .iter()
                .position(|c| *c == b'>')
                .ok_or(TzError::Name)?;
            if end < 3 {
                return Err(TzError::Name);
            }
            self.0 = &self.0[end + 1..];
            return Ok(());
        }
        let len = self
            .0
            .iter()
            .take_while(|c| c.is_ascii_alphabetic())
            .count();
        if len < 3 {
            return Err(TzError::Name);
        }
        self.0 = &self.0[len..];
        Ok(())
    }

    fn number(&mut self, max: u32) -> Result<u32, TzError> {
        let len = self.0.iter().take_while(|c| c.is_ascii_digit()).count();
        if len == 0 || len > 3 {
            return Err(TzError::Offset);
        }
        let n = self.0[..len]
            .iter()
            .fold(0, |n, c| n * 10 + u32::from(c - b'0'));
        self.0 = &self.0[len..];
        if n > max {
            return Err(TzError::Offset);
        }
        Ok(n)
    }

    /// `[+-]hh[:mm[:ss]]` in seconds; hours up to `max_hours`.
    fn time(&mut self, max_hours: u32) -> Result<i32, TzError> {
        let negative = if self.eat(b'-') {
            true
        } else {
            self.eat(b'+');
            false
        };
        let mut seconds = self.number(max_hours)? * 3600;
        if self.eat(b':') {
            seconds += self.number(59)? * 60;
            if self.eat(b':') {
                seconds += self.number(59)?;
            }
        }
        let seconds = i32::try_from(seconds).map_err(|_| TzError::Offset)?;
        Ok(if negative { -seconds } else { seconds })
    }

    fn offset(&mut self) -> Result<i32, TzError> {
        self.time(24)
    }

    fn rule(&mut self) -> Result<Rule, TzError> {
        let day = if self.eat(b'M') {
            let month = self.number(12).map_err(|_| TzError::Rule)?;
            let week = self
                .eat(b'.')
                .then(|| self.number(5))
                .ok_or(TzError::Rule)?
                .map_err(|_| TzError::Rule)?;
            let weekday = self
                .eat(b'.')
                .then(|| self.number(6))
                .ok_or(TzError::Rule)?
                .map_err(|_| TzError::Rule)?;
            if month == 0 || week == 0 {
                return Err(TzError::Rule);
            }
            #[allow(clippy::cast_possible_truncation)]
            Day::Weekday {
                month: month as u8,
                week: week as u8,
                weekday: weekday as u8,
            }
        } else if self.eat(b'J') {
            let n = self.number(365).map_err(|_| TzError::Rule)?;
            if n == 0 {
                return Err(TzError::Rule);
            }
            #[allow(clippy::cast_possible_truncation)]
            Day::Julian1(n as u16)
        } else {
            #[allow(clippy::cast_possible_truncation)]
            Day::Julian0(self.number(365).map_err(|_| TzError::Rule)? as u16)
        };
        let time = if self.eat(b'/') {
            self.time(167)?
        } else {
            2 * 3600
        };
        Ok(Rule { day, time })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::x509::time::DateTime;

    fn utc(y: i64, mo: u32, d: u32, h: u32, mi: u32) -> i64 {
        DateTime {
            year: y,
            month: mo,
            day: d,
            hour: h,
            minute: mi,
            second: 0,
        }
        .to_unix()
        .unwrap()
    }

    #[test]
    fn central_europe_switches_on_the_last_sundays() {
        let tz = TimeZone::parse(CENTRAL_EUROPE).unwrap();
        // 2026: 29 March 01:00 UTC and 25 October 01:00 UTC.
        assert_eq!(tz.offset_at(utc(2026, 3, 29, 0, 59)), 3600);
        assert_eq!(tz.offset_at(utc(2026, 3, 29, 1, 0)), 7200);
        assert_eq!(tz.offset_at(utc(2026, 10, 25, 0, 59)), 7200);
        assert_eq!(tz.offset_at(utc(2026, 10, 25, 1, 0)), 3600);
        assert_eq!(tz.offset_at(utc(2026, 1, 1, 12, 0)), 3600);
        assert_eq!(tz.offset_at(utc(2026, 7, 1, 12, 0)), 7200);
        // 2027: 28 March and 31 October.
        assert_eq!(tz.offset_at(utc(2027, 3, 28, 1, 0)), 7200);
        assert_eq!(tz.offset_at(utc(2027, 10, 31, 0, 59)), 7200);
        assert_eq!(tz.offset_at(utc(2027, 10, 31, 1, 0)), 3600);
    }

    #[test]
    fn local_and_back() {
        let tz = TimeZone::parse(CENTRAL_EUROPE).unwrap();
        let summer = utc(2026, 7, 1, 21, 0);
        assert_eq!(tz.to_local(summer), utc(2026, 7, 1, 23, 0));
        assert_eq!(tz.to_utc(utc(2026, 7, 1, 23, 0)), summer);
        let winter = utc(2026, 12, 1, 22, 0);
        assert_eq!(tz.to_local(winter), utc(2026, 12, 1, 23, 0));
        assert_eq!(tz.to_utc(utc(2026, 12, 1, 23, 0)), winter);
    }

    #[test]
    fn skipped_and_repeated_hours() {
        let tz = TimeZone::parse(CENTRAL_EUROPE).unwrap();
        // 02:30 on 29 March 2026 doesn't exist: read as 02:30 CET = 01:30 UTC.
        assert_eq!(tz.to_utc(utc(2026, 3, 29, 2, 30)), utc(2026, 3, 29, 1, 30));
        // 02:30 on 25 October 2026 exists twice: the first, CEST = 00:30 UTC.
        assert_eq!(
            tz.to_utc(utc(2026, 10, 25, 2, 30)),
            utc(2026, 10, 25, 0, 30)
        );
    }

    #[test]
    fn without_daylight_time() {
        let tz = TimeZone::parse("<+03>-3").unwrap();
        assert_eq!(tz.offset_at(utc(2026, 7, 1, 0, 0)), 3 * 3600);
        assert_eq!(TimeZone::parse("UTC0").unwrap(), TimeZone::UTC);
    }

    #[test]
    fn us_eastern_with_default_rules() {
        let tz = TimeZone::parse("EST5EDT").unwrap();
        // 8 March 2026 07:00 UTC and 1 November 2026 06:00 UTC.
        assert_eq!(tz.offset_at(utc(2026, 3, 8, 6, 59)), -5 * 3600);
        assert_eq!(tz.offset_at(utc(2026, 3, 8, 7, 0)), -4 * 3600);
        assert_eq!(tz.offset_at(utc(2026, 11, 1, 5, 59)), -4 * 3600);
        assert_eq!(tz.offset_at(utc(2026, 11, 1, 6, 0)), -5 * 3600);
    }

    #[test]
    fn southern_hemisphere() {
        // New Zealand: daylight from the last Sunday in September to the
        // first Sunday in April.
        let tz = TimeZone::parse("NZST-12NZDT,M9.5.0,M4.1.0/3").unwrap();
        assert_eq!(tz.offset_at(utc(2026, 1, 15, 0, 0)), 13 * 3600);
        assert_eq!(tz.offset_at(utc(2026, 7, 15, 0, 0)), 12 * 3600);
        assert_eq!(tz.offset_at(utc(2026, 12, 15, 0, 0)), 13 * 3600);
    }

    #[test]
    fn julian_rules() {
        // Daylight from day 60 (1 March, J) to day 300 (n), both 02:00.
        let tz = TimeZone::parse("XST0XDT,J60,300").unwrap();
        assert_eq!(tz.offset_at(utc(2028, 2, 29, 12, 0)), 0);
        assert_eq!(tz.offset_at(utc(2028, 3, 1, 2, 0)), 3600);
    }

    #[test]
    fn refuses_malformed_strings() {
        assert_eq!(TimeZone::parse(""), Err(TzError::Name));
        assert_eq!(TimeZone::parse("CE-1"), Err(TzError::Name));
        assert_eq!(TimeZone::parse("CET"), Err(TzError::Offset));
        assert_eq!(TimeZone::parse("CET-1CEST,M3.5.0"), Err(TzError::Rule));
        assert_eq!(
            TimeZone::parse("CET-1CEST,M13.5.0,M10.5.0"),
            Err(TzError::Rule)
        );
        assert_eq!(
            TimeZone::parse("CET-1CEST,M3.5.0,M10.5.0/3x"),
            Err(TzError::Trailing)
        );
        assert_eq!(TimeZone::parse(&"A".repeat(65)), Err(TzError::TooLong));
    }
}
