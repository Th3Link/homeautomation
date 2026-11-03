//! `/api/v1/logs`: the latest log lines ([`crate::log_buffer`]), and the
//! switch that mirrors the CAN traffic to MQTT ([`crate::can::MQTT_LOG`]).

use super::error::ApiError;
use super::json::json;
use core::sync::atomic::Ordering;
use picoserve::extract::{Json, Query};
use picoserve::response::IntoResponse;
use picoserve::routing::{get, PathRouter};
use picoserve::Router;
use serde::{Deserialize, Serialize};

/// Log bytes per answer; escaped as JSON they still fit the response
/// buffer.
const CHUNK: usize = 1200;

pub fn routes<R: PathRouter>(router: Router<R>) -> Router<impl PathRouter> {
    router.route("/logs", get(read).patch(update))
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
}

async fn read(Query(query): Query<From>) -> Result<impl IntoResponse, ApiError> {
    let (from, text, next) = crate::log_buffer::read::<CHUNK>(query.from);
    json(&Logs {
        from,
        text,
        next,
        can_to_mqtt: crate::can::MQTT_LOG.load(Ordering::Relaxed),
    })
    .await
}

#[derive(Deserialize)]
struct Update {
    can_to_mqtt: bool,
}

async fn update(Json(body): Json<Update>) -> Result<impl IntoResponse, ApiError> {
    crate::can::MQTT_LOG.store(body.can_to_mqtt, Ordering::Relaxed);
    crate::console_log!(
        "can: frames to MQTT {}",
        if body.can_to_mqtt { "on" } else { "off" }
    );
    read(Query(From { from: u32::MAX })).await
}
