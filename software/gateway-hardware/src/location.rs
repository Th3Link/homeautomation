//! Where the gateway is and its time zone, for switching by local time and
//! by the sun.
//!
//! Both live in the config store and are cached here, so readers don't
//! scan the flash. Without a place there are no sun times; without a time
//! zone the gateway uses Central European Time.

use crate::config::config;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use gateway_core::config::Key;
use gateway_core::sun::Place;
use gateway_core::tz::{TimeZone, CENTRAL_EUROPE, TZ_CAP};
use heapless::String;

/// The gateway's place and time zone.
#[derive(Debug, Clone)]
pub struct Location {
    pub place: Option<Place>,
    pub tz: TimeZone,
    /// `tz` as it was given.
    pub tz_text: String<TZ_CAP>,
}

impl Location {
    fn central_europe() -> Self {
        let mut tz_text = String::new();
        let _ = tz_text.push_str(CENTRAL_EUROPE);
        Self {
            place: None,
            tz: TimeZone::parse(CENTRAL_EUROPE).unwrap_or(TimeZone::UTC),
            tz_text,
        }
    }
}

static LOCATION: Mutex<CriticalSectionRawMutex, Option<Location>> = Mutex::new(None);

/// Reads the place and the time zone from the config store.
pub async fn init() {
    let mut location = Location::central_europe();
    {
        let mut store = config().await;
        let latitude = store.get_u32(Key::Latitude).await;
        let longitude = store.get_u32(Key::Longitude).await;
        if let (Some(latitude), Some(longitude)) = (latitude, longitude) {
            #[allow(clippy::cast_possible_wrap)]
            let place = Place::from_micro(latitude as i32, longitude as i32);
            location.place = place.is_valid().then_some(place);
        }
        if let Some(text) = store.get_str::<TZ_CAP>(Key::TimeZone).await {
            if let Ok(tz) = TimeZone::parse(&text) {
                location.tz = tz;
                location.tz_text = text;
            }
        }
    }
    *LOCATION.lock().await = Some(location);
}

/// The current place and time zone.
pub async fn get() -> Location {
    LOCATION
        .lock()
        .await
        .clone()
        .unwrap_or_else(Location::central_europe)
}

/// Stores a new place and/or time zone (checked by the caller).
pub async fn set(place: Option<Place>, tz: Option<&str>) -> Result<(), ()> {
    let mut location = get().await;
    {
        let mut store = config().await;
        if let Some(place) = place {
            let (latitude, longitude) = place.to_micro();
            #[allow(clippy::cast_sign_loss)]
            {
                store.set_u32(Key::Latitude, latitude as u32).await?;
                store.set_u32(Key::Longitude, longitude as u32).await?;
            }
            location.place = Some(place);
        }
        if let Some(text) = tz {
            location.tz = TimeZone::parse(text).map_err(|_| ())?;
            store.set_str(Key::TimeZone, text).await?;
            location.tz_text.clear();
            let _ = location.tz_text.push_str(text);
        }
    }
    *LOCATION.lock().await = Some(location);
    Ok(())
}
