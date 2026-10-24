//! `GET /api/v1/sensors`: every sensor seen on the bus and its latest
//! reading.

use super::error::ApiError;
use super::json::acquire;
use crate::device_list::DEVICE_LIST;
use crate::sensor_list::{now_seconds, SENSOR_LIST};
use core::fmt::Write;
use heapless::{String, Vec};
use picoserve::extract::Query;
use picoserve::response::IntoResponse;
use picoserve::routing::{get, PathRouter};
use picoserve::Router;
use serde::{Deserialize, Serialize};

/// Sensors per page.
const PAGE: usize = 16;

pub fn routes<R: PathRouter>(router: Router<R>) -> Router<impl PathRouter> {
    router.route("/sensors", get(list))
}

#[derive(Deserialize)]
struct ListQuery {
    /// Only the sensors of this device (name or `0x…` key).
    #[serde(default)]
    device: Option<String<12>>,
    #[serde(default)]
    offset: usize,
}

#[derive(Serialize)]
struct Sensor {
    /// The device the sensor is on: its name, or its key if it has none.
    device: String<12>,
    device_key: String<10>,
    /// `temperature`, `humidity`, `pressure`, `co2`, `voc` or `air_quality`.
    kind: &'static str,
    unit: &'static str,
    /// The sensor's own id; for a DS18B20 its 1-Wire address.
    sensor_id: String<16>,
    value: f32,
    age_s: u32,
}

#[derive(Serialize)]
struct Page {
    total: usize,
    offset: usize,
    sensors: Vec<Sensor, PAGE>,
}

async fn list(Query(query): Query<ListQuery>) -> Result<impl IntoResponse, ApiError> {
    let buffer = acquire().await;
    let now = now_seconds();

    let wanted_key = match &query.device {
        Some(name) => Some(
            DEVICE_LIST
                .lock()
                .await
                .resolve(name)
                .ok_or(ApiError::not_found("no such device"))?,
        ),
        None => None,
    };

    let (total, entries) = {
        let table = SENSOR_LIST.lock().await;
        let matching = || {
            table
                .iter()
                .filter(|e| wanted_key.is_none_or(|key| e.device_key == key))
        };
        let entries: Vec<_, PAGE> = matching().skip(query.offset).take(PAGE).copied().collect();
        (matching().count(), entries)
    };

    let mut sensors = Vec::new();
    {
        let devices = DEVICE_LIST.lock().await;
        for entry in &entries {
            let mut device_key = String::new();
            let _ = write!(device_key, "0x{:08x}", entry.device_key);
            let name = devices.entry(entry.device_key);
            let mut sensor_id = String::new();
            let _ = write!(sensor_id, "0x{:012x}", entry.sub_id);
            let _ = sensors.push(Sensor {
                device: String::try_from(if name.is_empty() {
                    device_key.as_str()
                } else {
                    name
                })
                .unwrap_or_default(),
                device_key,
                kind: entry.kind.name(),
                unit: entry.kind.unit(),
                sensor_id,
                value: entry.value,
                age_s: now.saturating_sub(entry.last_seen_s),
            });
        }
    }

    buffer.render(&Page {
        total,
        offset: query.offset,
        sensors,
    })
}
