//! `GET /api/v1/sensors`: every sensor seen on the bus, its latest reading
//! and what its node says about it (slot, name, role); and the commands that
//! name sensors and give them roles
//! (`/devices/:device/sensors/…`, see [`gateway_core::sensor_config`]).

use super::commands::{device_key, send_all};
use super::devices::DeviceName;
use super::error::ApiError;
use super::json::acquire;
use crate::device_list::DEVICE_LIST;
use crate::sensor_list::{now_seconds, SENSOR_INFO, SENSOR_LIST};
use core::fmt::Write;
use gateway_core::can_message_type::CanMessageType;
use gateway_core::command::{self, Target, MAX_SENDS};
use gateway_core::sensor_config::{SensorConfig, SensorName, SensorRole};
use heapless::{String, Vec};
use picoserve::extract::{Json, Query};
use picoserve::response::{IntoResponse, StatusCode};
use picoserve::routing::{get, parse_path_segment, post, put, PathRouter};
use picoserve::Router;
use serde::{Deserialize, Serialize};

/// Sensors per page.
const PAGE: usize = 16;

pub fn routes<R: PathRouter>(router: Router<R>) -> Router<impl PathRouter> {
    router
        .route("/sensors", get(list))
        .route(
            (
                "/devices",
                parse_path_segment::<DeviceName>(),
                "/sensors/refresh",
            ),
            post(refresh),
        )
        .route(
            (
                "/devices",
                parse_path_segment::<DeviceName>(),
                "/sensors/forget",
            ),
            post(forget),
        )
        .route(
            (
                "/devices",
                parse_path_segment::<DeviceName>(),
                "/sensors",
                parse_path_segment::<u8>(),
            ),
            put(update),
        )
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
    /// The sensor's slot on its node (temporary, see the protocol docs), if
    /// the node has told.
    slot: Option<u8>,
    /// Its name, if it has one.
    name: Option<String<7>>,
    /// `none`, `tank0` … `tank6` or `flow`, if the node has told.
    role: Option<&'static str>,
    lost: bool,
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
        let info = SENSOR_INFO.lock().await;
        for entry in &entries {
            let known = info.find(entry.device_key, entry.sub_id);
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
                slot: known.map(|k| k.slot),
                name: known
                    .and_then(|k| k.name)
                    .and_then(|name| String::try_from(name.as_str()).ok()),
                role: known.map(|k| k.role.name()),
                lost: known.is_some_and(|k| k.lost),
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

/// Asks the node for its sensor list again: the answer fills the slots, names
/// and roles shown by `GET /sensors`.
async fn refresh(name: DeviceName) -> Result<impl IntoResponse, ApiError> {
    let key = device_key(&name).await?;
    let sends = command::targeted_sends(
        &Target::CanByUid(key),
        CanMessageType::SensorConfig,
        &[],
        true,
    );
    send_all(sends).await
}

/// Makes the node forget the sensors that are lost.
async fn forget(name: DeviceName) -> Result<impl IntoResponse, ApiError> {
    let key = device_key(&name).await?;
    let payload = SensorConfig::Forget
        .to_payload()
        .map_err(|_| ApiError::invalid("cannot be sent"))?;
    let sends = command::targeted_sends(
        &Target::CanByUid(key),
        CanMessageType::SensorConfig,
        payload.as_slice(),
        false,
    );
    send_all(sends).await
}

/// Every field is optional; only the given ones are sent.
#[derive(Deserialize)]
struct Update {
    /// 1-7 characters out of `A-Z a-z 0-9 - _`.
    name: Option<String<8>>,
    /// `none`, `tank0` … `tank6` or `flow`.
    role: Option<String<8>>,
}

/// Names a sensor and/or gives it a role. The slot has to be one the node
/// has reported (`POST …/sensors/refresh` first if in doubt): its serial is
/// sent along as a guard, so that a slot that has moved is refused by the
/// node instead of renaming the wrong sensor.
async fn update(
    (name, slot): (DeviceName, u8),
    Json(body): Json<Update>,
) -> Result<impl IntoResponse, ApiError> {
    let key = device_key(&name).await?;
    let new_name = match &body.name {
        Some(text) => Some(SensorName::new(text).ok_or(ApiError::invalid(
            "name: 1-7 characters out of A-Z a-z 0-9 - _",
        ))?),
        None => None,
    };
    let new_role = match &body.role {
        Some(text) => Some(
            SensorRole::from_name(text)
                .ok_or(ApiError::invalid("role: none, tank0 … tank6 or flow"))?,
        ),
        None => None,
    };
    let serial = SENSOR_INFO
        .lock()
        .await
        .get(key, slot)
        .map(|info| info.serial)
        .ok_or(ApiError::new(
            StatusCode::CONFLICT,
            "unknown_slot",
            "the node has not reported this slot; refresh the sensor list first",
        ))?;

    let mut sends: Vec<command::CanSend, MAX_SENDS> = Vec::new();
    let target = Target::CanByUid(key);
    let mut add = |message: SensorConfig| -> Result<(), ApiError> {
        let payload = message
            .to_payload()
            .map_err(|_| ApiError::invalid("slot: 0-31"))?;
        let frames = command::targeted_sends(
            &target,
            CanMessageType::SensorConfig,
            payload.as_slice(),
            false,
        );
        sends
            .extend_from_slice(&frames)
            .map_err(|_| ApiError::invalid("too many frames"))
    };
    if let Some(name) = new_name {
        add(SensorConfig::Name { slot, name })?;
    }
    if let Some(role) = new_role {
        let guard = [serial[0], serial[1], serial[2], serial[3]];
        add(SensorConfig::Role { slot, role, guard })?;
    }
    send_all(sends).await
}
