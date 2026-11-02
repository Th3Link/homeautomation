//! Sunrise, sunset and twilight times, and the sun's elevation, for
//! switching by daylight.
//!
//! The algorithm of NOAA's solar calculator (Meeus' low-precision
//! formulas): the sun's declination and the equation of time, from which
//! the hour angle at the event's elevation, refined a few times with the
//! values at that time. Accurate to about a minute between ±72° latitude,
//! which is plenty for switching lights.
//!
//! Times are Unix seconds (UTC); the day is the UTC date whose solar noon
//! is meant, which for any place within ±12 h of Greenwich is its local
//! date. Latitude north and longitude east are positive, in degrees.

use crate::tz::TimeZone;
use crate::x509::time::DAY;
use libm::{acos, cos, sin, tan};

/// Julian date of the Unix epoch.
const UNIX_EPOCH_JD: f64 = 2_440_587.5;
/// Julian date of J2000.0.
const J2000: f64 = 2_451_545.0;
const MINUTES_PER_DAY: f64 = 1440.0;

/// A moment of the day defined by the sun.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    AstronomicalDawn,
    NauticalDawn,
    CivilDawn,
    Sunrise,
    Noon,
    Sunset,
    CivilDusk,
    NauticalDusk,
    AstronomicalDusk,
}

impl Event {
    pub const ALL: [Self; 9] = [
        Self::AstronomicalDawn,
        Self::NauticalDawn,
        Self::CivilDawn,
        Self::Sunrise,
        Self::Noon,
        Self::Sunset,
        Self::CivilDusk,
        Self::NauticalDusk,
        Self::AstronomicalDusk,
    ];

    /// The sun's elevation at the event, degrees: sunrise and sunset are
    /// when the upper limb touches the horizon (refraction included),
    /// twilight when the centre is 6°, 12° or 18° below it.
    pub fn elevation(self) -> f64 {
        match self {
            Self::Sunrise | Self::Sunset => -0.833,
            Self::CivilDawn | Self::CivilDusk => -6.0,
            Self::NauticalDawn | Self::NauticalDusk => -12.0,
            Self::AstronomicalDawn | Self::AstronomicalDusk => -18.0,
            Self::Noon => 90.0,
        }
    }

    fn morning(self) -> bool {
        matches!(
            self,
            Self::AstronomicalDawn | Self::NauticalDawn | Self::CivilDawn | Self::Sunrise
        )
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::AstronomicalDawn => "astronomical_dawn",
            Self::NauticalDawn => "nautical_dawn",
            Self::CivilDawn => "civil_dawn",
            Self::Sunrise => "sunrise",
            Self::Noon => "noon",
            Self::Sunset => "sunset",
            Self::CivilDusk => "civil_dusk",
            Self::NauticalDusk => "nautical_dusk",
            Self::AstronomicalDusk => "astronomical_dusk",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|e| e.name() == name)
    }
}

/// A place on Earth, degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Place {
    pub latitude: f64,
    pub longitude: f64,
}

impl Place {
    /// Whether the coordinates are on Earth.
    pub fn is_valid(self) -> bool {
        (-90.0..=90.0).contains(&self.latitude) && (-180.0..=180.0).contains(&self.longitude)
    }

    /// Latitude and longitude in micro-degrees (about 0.1 m), for storing.
    #[allow(clippy::cast_possible_truncation)]
    pub fn to_micro(self) -> (i32, i32) {
        let micro = |degrees: f64| libm::round(degrees * 1e6) as i32;
        (micro(self.latitude), micro(self.longitude))
    }

    pub fn from_micro(latitude: i32, longitude: i32) -> Self {
        Self {
            latitude: f64::from(latitude) / 1e6,
            longitude: f64::from(longitude) / 1e6,
        }
    }
}

/// Every [`Event`] of the local day `local_day` (days since the Unix
/// epoch, in local time), as local seconds, in [`Event::ALL`] order.
/// `None` where the sun doesn't reach the event's elevation.
pub fn local_events(local_day: i64, place: Place, tz: &TimeZone) -> [Option<i64>; 9] {
    Event::ALL.map(|e| event(local_day, place, e).map(|utc| tz.to_local(utc)))
}

/// `value` modulo `modulus`, in `0..modulus` (`f64::rem_euclid` needs
/// `std`).
fn wrap(value: f64, modulus: f64) -> f64 {
    let r = libm::fmod(value, modulus);
    if r < 0.0 {
        r + modulus
    } else {
        r
    }
}

/// The sun's declination (radians) and the equation of time (minutes) at
/// the Julian date `jd`.
fn declination_and_equation_of_time(jd: f64) -> (f64, f64) {
    let t = (jd - J2000) / 36525.0;
    let mean_longitude = wrap(280.466_46 + t * (36_000.769_83 + t * 0.000_303_2), 360.0);
    let mean_anomaly = 357.529_11 + t * (35_999.050_29 - 0.000_153_7 * t);
    let eccentricity = 0.016_708_634 - t * (0.000_042_037 + 0.000_000_126_7 * t);
    let m = mean_anomaly.to_radians();
    let centre = sin(m) * (1.914_602 - t * (0.004_817 + 0.000_014 * t))
        + sin(2.0 * m) * (0.019_993 - 0.000_101 * t)
        + sin(3.0 * m) * 0.000_289;
    let omega = (125.04 - 1934.136 * t).to_radians();
    let apparent = (mean_longitude + centre - 0.005_69 - 0.004_78 * sin(omega)).to_radians();
    let mean_obliquity =
        23.0 + (26.0 + (21.448 - t * (46.815 + t * (0.000_59 - t * 0.001_813))) / 60.0) / 60.0;
    let obliquity = (mean_obliquity + 0.002_56 * cos(omega)).to_radians();
    let declination = libm::asin(sin(obliquity) * sin(apparent));
    let y = tan(obliquity / 2.0) * tan(obliquity / 2.0);
    let l0 = mean_longitude.to_radians();
    let e = eccentricity;
    let equation_of_time = 4.0
        * (y * sin(2.0 * l0) - 2.0 * e * sin(m) + 4.0 * e * y * sin(m) * cos(2.0 * l0)
            - 0.5 * y * y * sin(4.0 * l0)
            - 1.25 * e * e * sin(2.0 * m))
        .to_degrees();
    (declination, equation_of_time)
}

/// When `event` happens at `place` on the day `day` (days since the Unix
/// epoch, UTC date). `None` if the sun doesn't reach that elevation that
/// day (midnight sun, polar night, white nights).
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
pub fn event(day: i64, place: Place, event: Event) -> Option<i64> {
    let midnight = day as f64 + UNIX_EPOCH_JD;
    let phi = place.latitude.to_radians();
    // Minutes after 00:00 UTC, starting from mean noon, refined with the
    // sun's values at the time found.
    let mut minutes = 720.0 - 4.0 * place.longitude;
    for _ in 0..4 {
        let (declination, equation_of_time) =
            declination_and_equation_of_time(midnight + minutes / MINUTES_PER_DAY);
        let noon = 720.0 - 4.0 * place.longitude - equation_of_time;
        if event == Event::Noon {
            minutes = noon;
            continue;
        }
        let cos_hour_angle = (sin(event.elevation().to_radians()) - sin(phi) * sin(declination))
            / (cos(phi) * cos(declination));
        if !(-1.0..=1.0).contains(&cos_hour_angle) {
            return None;
        }
        let hour_angle = acos(cos_hour_angle).to_degrees();
        minutes = if event.morning() {
            noon - 4.0 * hour_angle
        } else {
            noon + 4.0 * hour_angle
        };
    }
    Some(day * DAY + libm::round(minutes * 60.0) as i64)
}

/// The sun's elevation above the horizon at `place` and the instant `utc`,
/// degrees (without refraction).
#[allow(clippy::cast_precision_loss)]
pub fn elevation(utc: i64, place: Place) -> f64 {
    let jd = utc as f64 / DAY as f64 + UNIX_EPOCH_JD;
    let (declination, equation_of_time) = declination_and_equation_of_time(jd);
    let minutes = utc.rem_euclid(DAY) as f64 / 60.0;
    let true_solar = wrap(
        minutes + equation_of_time + 4.0 * place.longitude,
        MINUTES_PER_DAY,
    );
    let hour_angle = (true_solar / 4.0 - 180.0).to_radians();
    let phi = place.latitude.to_radians();
    libm::asin(sin(phi) * sin(declination) + cos(phi) * cos(declination) * cos(hour_angle))
        .to_degrees()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::x509::time::{days_from_civil, DateTime};

    const BERLIN: Place = Place {
        latitude: 52.52,
        longitude: 13.405,
    };

    fn hm(utc: i64) -> (u32, u32) {
        let t = DateTime::from_unix(utc);
        (t.hour, t.minute)
    }

    #[test]
    fn noon_is_near_twelve_local_mean_time() {
        let day = days_from_civil(2026, 6, 21);
        let noon = event(day, BERLIN, Event::Noon).unwrap();
        // 12:00 − 13.4°·4 min/° ≈ 11:06 UTC, plus the equation of time.
        assert_eq!(hm(noon), (11, 8));
    }

    #[test]
    fn berlin_matches_the_noaa_calculator() {
        // From NOAA's spreadsheet formulas (UTC, hh:mm:ss); they agree
        // with the published times (21 June: sunrise 04:43, sunset 21:33
        // CEST; 21 December: 08:15 and 15:54 CET).
        let reference = [
            (
                (1, 10),
                [(6, 33, 23), (7, 13, 43), (15, 14, 25), (15, 54, 45)],
            ),
            (
                (3, 21),
                [(4, 32, 50), (5, 6, 53), (17, 21, 13), (17, 55, 23)],
            ),
            (
                (6, 21),
                [(1, 52, 50), (2, 43, 5), (19, 33, 18), (20, 23, 33)],
            ),
            (
                (9, 23),
                [(4, 19, 40), (4, 53, 48), (17, 2, 44), (17, 36, 45)],
            ),
            (
                (12, 21),
                [(6, 33, 13), (7, 14, 55), (14, 53, 58), (15, 35, 40)],
            ),
        ];
        let events = [
            Event::CivilDawn,
            Event::Sunrise,
            Event::Sunset,
            Event::CivilDusk,
        ];
        for ((month, day), times) in reference {
            let d = days_from_civil(2026, month, day);
            for (e, (h, m, s)) in events.into_iter().zip(times) {
                let expected = d * DAY + i64::from(h * 3600 + m * 60 + s);
                let got = event(d, BERLIN, e).unwrap();
                assert!(
                    (got - expected).abs() <= 2,
                    "{month}-{day} {e:?}: {got} vs {expected}"
                );
            }
        }
    }

    #[test]
    fn events_come_in_order() {
        for (month, day) in [(1, 15), (3, 21), (6, 21), (9, 23), (12, 21)] {
            let d = days_from_civil(2026, month, day);
            let times: std::vec::Vec<i64> = [
                Event::NauticalDawn,
                Event::CivilDawn,
                Event::Sunrise,
                Event::Noon,
                Event::Sunset,
                Event::CivilDusk,
                Event::NauticalDusk,
            ]
            .into_iter()
            .map(|e| event(d, BERLIN, e).unwrap())
            .collect();
            assert!(times.windows(2).all(|w| w[0] < w[1]), "{month}-{day}");
        }
    }

    #[test]
    fn the_elevation_at_each_event_is_the_events() {
        // Two different formulas: the time from the hour angle, the
        // elevation from the sidereal time.
        for month in 1..=12 {
            let d = days_from_civil(2026, month, 10);
            for e in [
                Event::CivilDawn,
                Event::Sunrise,
                Event::Sunset,
                Event::CivilDusk,
                Event::NauticalDusk,
            ] {
                let at = event(d, BERLIN, e).unwrap();
                let off = (elevation(at, BERLIN) - e.elevation()).abs();
                assert!(off < 0.01, "{month} {e:?}: {off}");
            }
        }
    }

    #[test]
    fn day_length_at_the_solstices() {
        let length = |m, d| {
            let day = days_from_civil(2026, m, d);
            event(day, BERLIN, Event::Sunset).unwrap() - event(day, BERLIN, Event::Sunrise).unwrap()
        };
        // About 16 h 50 min in June and 7 h 40 min in December in Berlin.
        assert!((16 * 3600 + 45 * 60..16 * 3600 + 55 * 60).contains(&length(6, 21)));
        assert!((7 * 3600 + 35 * 60..7 * 3600 + 45 * 60).contains(&length(12, 21)));
    }

    #[test]
    fn polar_cases_have_no_time() {
        let tromso = Place {
            latitude: 69.65,
            longitude: 18.96,
        };
        assert_eq!(
            event(days_from_civil(2026, 6, 21), tromso, Event::Sunset),
            None
        );
        assert_eq!(
            event(days_from_civil(2026, 12, 21), tromso, Event::Sunrise),
            None
        );
        // Berlin has white nights in June: no astronomical dusk.
        assert_eq!(
            event(
                days_from_civil(2026, 6, 21),
                BERLIN,
                Event::AstronomicalDusk
            ),
            None
        );
    }

    #[test]
    fn local_events_in_central_european_time() {
        let tz = TimeZone::parse(crate::tz::CENTRAL_EUROPE).unwrap();
        let day = days_from_civil(2026, 6, 21);
        let events = local_events(day, BERLIN, &tz);
        // Sunrise 02:43 UTC is 04:43 CEST.
        assert_eq!(hm(events[3].unwrap()), (4, 43));
        assert_eq!(events[8], None);
    }

    #[test]
    fn micro_degrees_round_trip() {
        assert_eq!(BERLIN.to_micro(), (52_520_000, 13_405_000));
        assert_eq!(Place::from_micro(52_520_000, 13_405_000), BERLIN);
        let west = Place {
            latitude: -33.8688,
            longitude: -151.2093,
        };
        assert_eq!(
            Place::from_micro(west.to_micro().0, west.to_micro().1),
            west
        );
    }

    #[test]
    fn names_round_trip() {
        for e in Event::ALL {
            assert_eq!(Event::from_name(e.name()), Some(e));
        }
        assert_eq!(Event::from_name("dusk"), None);
    }

    #[test]
    fn place_bounds() {
        assert!(BERLIN.is_valid());
        assert!(!Place {
            latitude: 91.0,
            longitude: 0.0
        }
        .is_valid());
    }
}
