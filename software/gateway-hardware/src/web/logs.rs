//! `/api/v1/logs`: the latest log lines ([`crate::log_buffer`]), the switch
//! that mirrors the CAN traffic to MQTT ([`crate::can::MQTT_LOG`]) and the
//! log lines on MQTT ([`crate::node_log`]). `/api/v1/devices/:device/log`:
//! a node's log, downloaded over CAN.

use super::devices::DeviceName;
use super::error::ApiError;
use super::json::json;
use crate::device_list::DEVICE_LIST;
use core::sync::atomic::Ordering;
use picoserve::extract::{Json, Query};
use picoserve::response::{IntoResponse, Response, StatusCode};
use picoserve::routing::{get, parse_path_segment, PathRouter};
use picoserve::Router;
use serde::{Deserialize, Serialize};

/// Log bytes per answer; escaped as JSON they still fit the response
/// buffer.
const CHUNK: usize = 1200;

pub fn routes<R: PathRouter>(router: Router<R>) -> Router<impl PathRouter> {
    router.route("/logs", get(read).patch(update)).route(
        ("/devices", parse_path_segment::<DeviceName>(), "/log"),
        get(read_device).post(start_device),
    )
}

#[derive(Deserialize)]
struct From {
    #[serde(default)]
    from: u32,
}

#[derive(Serialize)]
struct Logs {
    /// Where `text` starts in the stream of everything logged since boot
    /// (later than asked for if that was overwritten).
    from: u32,
    /// Whole lines.
    text: heapless::String<CHUNK>,
    /// Where to ask from next.
    next: u32,
    /// Whether the CAN frames go to `canbus/log/0x<id>` too.
    can_to_mqtt: bool,
    /// Whether the log lines go to MQTT: the gateway's own and the nodes'.
    logs_to_mqtt: bool,
    /// Seconds between two rounds of node log downloads; 0: none.
    node_log_interval_s: u32,
}

async fn read(Query(query): Query<From>) -> Result<impl IntoResponse, ApiError> {
    let (from, text, next) = crate::log_buffer::read::<CHUNK>(query.from);
    json(&Logs {
        from,
        text,
        next,
        can_to_mqtt: crate::can::MQTT_LOG.load(Ordering::Relaxed),
        logs_to_mqtt: crate::node_log::to_mqtt(),
        node_log_interval_s: crate::node_log::interval_s(),
    })
    .await
}

/// Every field is optional; only the given ones change.
#[derive(Deserialize)]
struct Update {
    can_to_mqtt: Option<bool>,
    logs_to_mqtt: Option<bool>,
    node_log_interval_s: Option<u32>,
}

async fn update(Json(body): Json<Update>) -> Result<impl IntoResponse, ApiError> {
    if let Some(seconds) = body.node_log_interval_s {
        if seconds != 0 && !(60..=86_400).contains(&seconds) {
            return Err(ApiError::invalid(
                "node_log_interval_s: 0 (off) or 60-86400",
            ));
        }
    }
    if let Some(on) = body.can_to_mqtt {
        crate::can::MQTT_LOG.store(on, Ordering::Relaxed);
        crate::console_log!("can: frames to MQTT {}", if on { "on" } else { "off" });
    }
    let failed = ApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "storage",
        "could not store the setting",
    );
    if let Some(on) = body.logs_to_mqtt {
        crate::node_log::set_to_mqtt(on)
            .await
            .map_err(|()| failed)?;
    }
    if let Some(seconds) = body.node_log_interval_s {
        crate::node_log::set_interval_s(seconds)
            .await
            .map_err(|()| failed)?;
    }
    read(Query(From { from: u32::MAX })).await
}

#[derive(Serialize)]
struct DeviceLog<'a> {
    /// `none` (not downloaded since boot, or another device was),
    /// `running`, `done`, `no_answer` or `failed`.
    state: &'static str,
    /// Where `text` starts in what was downloaded.
    from: u32,
    /// Whole lines.
    text: &'a str,
    /// Where to ask from next.
    next: u32,
    /// Bytes the node overwrote before they were downloaded.
    lost_bytes: u32,
}

async fn device_key(name: &DeviceName) -> Result<u32, ApiError> {
    DEVICE_LIST
        .lock()
        .await
        .resolve(name.as_str())
        .ok_or(ApiError::not_found("no such device"))
}

/// What was downloaded from the device, from `from` on.
async fn read_device(
    name: DeviceName,
    Query(query): Query<From>,
) -> Result<impl IntoResponse, ApiError> {
    crate::heap::boxed(|| async move {
        let key = device_key(&name).await?;
        let buffer = super::json::acquire().await;
        let body = crate::node_log::with_web(|web| {
            let Some(web) = web.filter(|web| web.device_key == key) else {
                return buffer.rendered(&DeviceLog {
                    state: "none",
                    from: 0,
                    text: "",
                    next: 0,
                    lost_bytes: 0,
                });
            };
            // Whole lines, at most a chunk.
            let from = (query.from as usize).min(web.text.len());
            let rest = web.text.get(from..).unwrap_or("");
            let end = match rest.get(..CHUNK.min(rest.len())) {
                Some(chunk) if chunk.len() < rest.len() => chunk.rfind('\n').map_or(0, |i| i + 1),
                _ => rest.len(),
            };
            buffer.rendered(&DeviceLog {
                state: web.state.name(),
                from: from as u32,
                text: &rest[..end],
                next: (from + end) as u32,
                lost_bytes: web.lost,
            })
        })
        .await?;
        Ok(Response::new(StatusCode::OK, body))
    })
    .await
}

/// Downloads the device's log (`202`); `GET` shows it.
async fn start_device(name: DeviceName) -> Result<impl IntoResponse, ApiError> {
    crate::heap::boxed(|| async move {
        let key = device_key(&name).await?;
        if !crate::node_log::request(key).await {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "busy",
                "a download is running",
            ));
        }
        Ok(StatusCode::ACCEPTED)
    })
    .await
}
