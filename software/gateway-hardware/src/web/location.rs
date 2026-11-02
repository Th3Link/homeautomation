//! `/api/v1/location`: where the gateway is, its time zone, and today's
//! sun times there.

use super::error::ApiError;
use super::json::json;
use crate::location;
use core::fmt::Write;
use gateway_core::sun::{local_events, Place};
use gateway_core::tz::{TimeZone, TZ_CAP};
use gateway_core::x509::time::{DateTime, DAY};
use heapless::String;
use picoserve::extract::Json;
use picoserve::response::{IntoResponse, StatusCode};
use picoserve::routing::{get, PathRouter};
use picoserve::Router;
use serde::{Deserialize, Serialize};

pub fn routes<R: PathRouter>(router: Router<R>) -> Router<impl PathRouter> {
    router.route("/location", get(read).patch(update))
}

/// `HH:MM` local time.
type Clock = String<5>;

#[derive(Serialize)]
struct SunTimes {
    astronomical_dawn: Option<Clock>,
    nautical_dawn: Option<Clock>,
    civil_dawn: Option<Clock>,
    sunrise: Option<Clock>,
    noon: Option<Clock>,
    sunset: Option<Clock>,
    civil_dusk: Option<Clock>,
    nautical_dusk: Option<Clock>,
    astronomical_dusk: Option<Clock>,
}

#[derive(Serialize)]
struct Location {
    latitude: Option<f64>,
    longitude: Option<f64>,
    timezone: String<TZ_CAP>,
    /// `YYYY-MM-DDTHH:MM:SS`, local; `null` until the clock is set.
    local_time: Option<String<19>>,
    utc_offset_minutes: Option<i32>,
    /// Today's sun times, local; `null` without a place or a clock.
    sun: Option<SunTimes>,
}

fn clock(local: i64) -> Clock {
    let t = DateTime::from_unix(local);
    let mut s = Clock::new();
    let _ = write!(s, "{:02}:{:02}", t.hour, t.minute);
    s
}

async fn read() -> Result<impl IntoResponse, ApiError> {
    let location = location::get().await;
    let now = crate::wall_clock::now();
    let local = now.map(|utc| location.tz.to_local(utc));
    let local_time = local.map(|local| {
        let t = DateTime::from_unix(local);
        let mut s = String::new();
        let _ = write!(
            s,
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            t.year, t.month, t.day, t.hour, t.minute, t.second
        );
        s
    });
    let sun = match (location.place, local) {
        (Some(place), Some(local)) => {
            let [astronomical_dawn, nautical_dawn, civil_dawn, sunrise, noon, sunset, civil_dusk, nautical_dusk, astronomical_dusk] =
                local_events(local.div_euclid(DAY), place, &location.tz).map(|t| t.map(clock));
            Some(SunTimes {
                astronomical_dawn,
                nautical_dawn,
                civil_dawn,
                sunrise,
                noon,
                sunset,
                civil_dusk,
                nautical_dusk,
                astronomical_dusk,
            })
        }
        _ => None,
    };
    json(&Location {
        latitude: location.place.map(|p| p.latitude),
        longitude: location.place.map(|p| p.longitude),
        timezone: location.tz_text,
        local_time,
        utc_offset_minutes: now.map(|utc| location.tz.offset_at(utc) / 60),
        sun,
    })
    .await
}

/// Every field is optional; latitude and longitude only together.
#[derive(Deserialize)]
struct Update {
    latitude: Option<f64>,
    longitude: Option<f64>,
    timezone: Option<String<TZ_CAP>>,
}

async fn update(Json(body): Json<Update>) -> Result<impl IntoResponse, ApiError> {
    let place = match (body.latitude, body.longitude) {
        (Some(latitude), Some(longitude)) => {
            let place = Place {
                latitude,
                longitude,
            };
            if !place.is_valid() {
                return Err(ApiError::invalid(
                    "latitude: -90 to 90, longitude: -180 to 180",
                ));
            }
            Some(place)
        }
        (None, None) => None,
        _ => return Err(ApiError::invalid("latitude and longitude only together")),
    };
    if let Some(tz) = &body.timezone {
        if TimeZone::parse(tz).is_err() {
            return Err(ApiError::invalid(
                "timezone: a POSIX TZ string, e.g. CET-1CEST,M3.5.0,M10.5.0/3",
            ));
        }
    }
    location::set(place, body.timezone.as_deref())
        .await
        .map_err(|()| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage",
                "could not store the setting",
            )
        })?;
    read().await
}
