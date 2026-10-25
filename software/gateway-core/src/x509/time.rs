//! Calendar arithmetic for certificate validity times: Unix seconds to and
//! from the ASN.1 `UTCTime`/`GeneralizedTime` and RFC 3339 forms (UTC only,
//! which is all a certificate or a build timestamp uses).

use super::der::{tag, Overflow, Writer};
use core::fmt;

pub const DAY: i64 = 86_400;

/// Days since 1970-01-01 of a proleptic Gregorian date (H. Hinnant's
/// `days_from_civil`).
pub fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let month = i64::from(month);
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The date of a day count since 1970-01-01, as `(year, month, day)`.
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// A UTC date and time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateTime {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

impl DateTime {
    pub fn from_unix(seconds: i64) -> Self {
        let (year, month, day) = civil_from_days(seconds.div_euclid(DAY));
        let secs = seconds.rem_euclid(DAY) as u32;
        Self {
            year,
            month,
            day,
            hour: secs / 3600,
            minute: secs / 60 % 60,
            second: secs % 60,
        }
    }

    /// `None` if a field is out of range.
    pub fn to_unix(self) -> Option<i64> {
        let days_in_month = match self.month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if self.year % 4 == 0 && (self.year % 100 != 0 || self.year % 400 == 0) => 29,
            2 => 28,
            _ => return None,
        };
        if self.day == 0
            || self.day > days_in_month
            || self.hour > 23
            || self.minute > 59
            || self.second > 60
        {
            return None;
        }
        Some(
            days_from_civil(self.year, self.month, self.day) * DAY
                + i64::from(self.hour * 3600 + self.minute * 60 + self.second.min(59)),
        )
    }
}

fn digits(text: &[u8]) -> Option<u32> {
    if text.is_empty() || !text.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some(text.iter().fold(0, |n, d| n * 10 + u32::from(d - b'0')))
}

/// Parses the content of a `UTCTime` (`YYMMDDHHMMSSZ`) or `GeneralizedTime`
/// (`YYYYMMDDHHMMSSZ`) element, as RFC 5280 requires them.
pub fn parse_asn1(tag: u8, content: &[u8]) -> Option<i64> {
    let (year, rest) = match tag {
        tag::UTC_TIME if content.len() == 13 => {
            let yy = i64::from(digits(&content[..2])?);
            (if yy < 50 { 2000 + yy } else { 1900 + yy }, &content[2..])
        }
        tag::GENERALIZED_TIME if content.len() == 15 => {
            (i64::from(digits(&content[..4])?), &content[4..])
        }
        _ => return None,
    };
    if rest[10] != b'Z' {
        return None;
    }
    DateTime {
        year,
        month: digits(&rest[0..2])?,
        day: digits(&rest[2..4])?,
        hour: digits(&rest[4..6])?,
        minute: digits(&rest[6..8])?,
        second: digits(&rest[8..10])?,
    }
    .to_unix()
}

/// Writes a validity time the way RFC 5280 wants it: `UTCTime` up to 2049,
/// `GeneralizedTime` from 2050 on.
pub fn write_asn1(w: &mut Writer<'_>, seconds: i64) -> Result<(), Overflow> {
    let t = DateTime::from_unix(seconds);
    let mut text = heapless::String::<16>::new();
    let (tag, year) = if (1950..2050).contains(&t.year) {
        (tag::UTC_TIME, t.year % 100)
    } else {
        (tag::GENERALIZED_TIME, t.year)
    };
    let width = if tag == tag::UTC_TIME { 2 } else { 4 };
    fmt::write(
        &mut text,
        format_args!(
            "{year:0width$}{:02}{:02}{:02}{:02}{:02}Z",
            t.month, t.day, t.hour, t.minute, t.second
        ),
    )
    .map_err(|_| Overflow)?;
    w.element(tag, text.as_bytes())
}

/// Parses the start of an RFC 3339 timestamp, `YYYY-MM-DDTHH:MM:SS`, as UTC
/// (fractions and a trailing `Z` are ignored; build timestamps are UTC).
pub fn parse_rfc3339(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || b[13] != b':' || b[16] != b':' {
        return None;
    }
    if b[10] != b'T' && b[10] != b' ' {
        return None;
    }
    DateTime {
        year: i64::from(digits(&b[0..4])?),
        month: digits(&b[5..7])?,
        day: digits(&b[8..10])?,
        hour: digits(&b[11..13])?,
        minute: digits(&b[14..16])?,
        second: digits(&b[17..19])?,
    }
    .to_unix()
}

/// Formats Unix seconds as `YYYY-MM-DDTHH:MM:SSZ`.
pub struct Rfc3339(pub i64);

impl fmt::Display for Rfc3339 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let t = DateTime::from_unix(self.0);
        write!(
            f,
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            t.year, t.month, t.day, t.hour, t.minute, t.second
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::fmt::Write;

    #[test]
    fn known_dates() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(civil_from_days(11_017), (2000, 3, 1));
        // 2026-10-25T12:00:00Z
        assert_eq!(
            DateTime {
                year: 2026,
                month: 10,
                day: 25,
                hour: 12,
                minute: 0,
                second: 0
            }
            .to_unix(),
            Some(1_792_929_600)
        );
        for days in [-1000, 0, 59, 60, 365, 10_957, 20_000, 30_000, 60_000] {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days);
        }
    }

    #[test]
    fn asn1_times_round_trip() {
        for (seconds, tag) in [
            (1_792_929_600, tag::UTC_TIME),
            (2_524_608_000, tag::GENERALIZED_TIME), // 2050-01-01
            (2_524_607_999, tag::UTC_TIME),
        ] {
            let mut buf = [0u8; 32];
            let mut w = Writer::new(&mut buf);
            write_asn1(&mut w, seconds).unwrap();
            let bytes = w.as_bytes();
            assert_eq!(bytes[0], tag);
            assert_eq!(parse_asn1(bytes[0], &bytes[2..]), Some(seconds));
        }
        assert_eq!(
            parse_asn1(tag::UTC_TIME, b"261025120000Z"),
            Some(1_792_929_600)
        );
        assert_eq!(
            parse_asn1(tag::UTC_TIME, b"500101000000Z"),
            Some(-631_152_000)
        );
    }

    #[test]
    fn bad_times_are_rejected() {
        assert_eq!(parse_asn1(tag::UTC_TIME, b"261325120000Z"), None);
        assert_eq!(parse_asn1(tag::UTC_TIME, b"260230120000Z"), None);
        assert_eq!(parse_asn1(tag::UTC_TIME, b"2610251200Z"), None);
        assert_eq!(parse_asn1(tag::UTC_TIME, b"261025120000+"), None);
        assert_eq!(parse_asn1(tag::OCTET_STRING, b"261025120000Z"), None);
    }

    #[test]
    fn rfc3339() {
        assert_eq!(
            parse_rfc3339("2026-10-25T12:00:00.123456789Z"),
            Some(1_792_929_600)
        );
        assert_eq!(parse_rfc3339("2026-10-25T12:00:00Z"), Some(1_792_929_600));
        assert_eq!(parse_rfc3339("2026-10-25"), None);
        let mut text = heapless::String::<24>::new();
        write!(text, "{}", Rfc3339(1_792_929_600)).unwrap();
        assert_eq!(text, "2026-10-25T12:00:00Z");
    }
}
