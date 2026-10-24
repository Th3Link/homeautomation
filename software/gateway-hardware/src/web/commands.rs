//! Routes that put commands on the CAN bus: per device (`/devices/:device/…`),
//! per device type (`/device-types/:type/…`) and to everybody (`/bus/…`).
//! The CAN frames come from [`gateway_core::command`], the same functions the
//! original `/control.json` verbs map to.
//!
//! A command is answered with `202 Accepted` and the number of frames put on
//! the bus: the bus has no acknowledgement, the result shows up in the
//! device's state like any other change.

use super::devices::DeviceName;
use super::error::ApiError;
use super::json::with_status;
use crate::device_list::DEVICE_LIST;
use gateway_core::can_message_type::CanMessageType;
use gateway_core::command::{self, CanSend, SaveDeviceFields, Target, MAX_SENDS};
use gateway_core::lamp::LampMsg;
use gateway_core::relais::RelaisMsg;
use gateway_core::web::{
    celsius_to_raw, parse_hex_u64, parse_relay_state, strict_extension_mode, strict_relais_mode,
};
use heapless::{String, Vec};
use picoserve::response::{IntoResponse, Json, StatusCode};
use picoserve::routing::{parse_path_segment, post, put, PathRouter};
use picoserve::Router;
use serde::{Deserialize, Serialize};

const DEVICES: &str = "/devices";

/// Adds the command routes to the API router.
pub fn routes<R: PathRouter>(router: Router<R>) -> Router<impl PathRouter> {
    let device = || parse_path_segment::<DeviceName>();
    router
        .route((DEVICES, device(), "/ping"), post(ping))
        .route((DEVICES, device(), "/refresh"), post(refresh))
        .route((DEVICES, device(), "/restart"), post(restart))
        .route((DEVICES, device(), "/legacy-mode"), post(legacy_mode))
        .route((DEVICES, device(), "/silence"), put(silence))
        .route(
            (DEVICES, device(), "/relays", parse_path_segment::<u8>()),
            put(relay),
        )
        .route(
            (
                DEVICES,
                device(),
                "/rollershutters",
                parse_path_segment::<u8>(),
            ),
            put(rollershutter),
        )
        .route(
            (DEVICES, device(), "/lamps", parse_path_segment::<u8>()),
            put(lamps),
        )
        .route((DEVICES, device(), "/heating/manual"), put(heating_manual))
        .route((DEVICES, device(), "/heating/target"), put(heating_target))
        .route((DEVICES, device(), "/heating/energy"), put(heating_energy))
        .route(
            (DEVICES, device(), "/heating/emergency"),
            put(heating_emergency),
        )
        .route(
            ("/device-types", parse_path_segment::<u8>(), "/ping"),
            post(type_ping),
        )
        .route(
            ("/device-types", parse_path_segment::<u8>(), "/refresh"),
            post(type_refresh),
        )
        .route(
            ("/device-types", parse_path_segment::<u8>(), "/restart"),
            post(type_restart),
        )
        .route("/bus/ping", post(bus_ping))
        .route("/bus/refresh", post(bus_refresh))
        .route("/bus/restart", post(bus_restart))
        .route("/bus/silence", put(bus_silence))
}

/// `PATCH /devices/:device` is registered with the device's `GET`.
pub use save as patch_device;

#[derive(Serialize)]
struct Sent {
    sent: usize,
}

async fn send_all(sends: Vec<CanSend, MAX_SENDS>) -> Result<impl IntoResponse, ApiError> {
    let sent = sends.len();
    for send in sends {
        crate::can::send_raw(send.can_id, &send.data, send.request).await;
    }
    with_status(StatusCode::ACCEPTED, &Sent { sent }).await
}

async fn device_key(name: &DeviceName) -> Result<u32, ApiError> {
    DEVICE_LIST
        .lock()
        .await
        .resolve(name.as_str())
        .ok_or(ApiError::not_found("no such device"))
}

async fn target(name: &DeviceName) -> Result<Target, ApiError> {
    Ok(Target::CanByUid(device_key(name).await?))
}

// ---- single device -------------------------------------------------------

async fn ping(name: DeviceName) -> Result<impl IntoResponse, ApiError> {
    send_all(command::ping_sends(&target(&name).await?)).await
}

async fn refresh(name: DeviceName) -> Result<impl IntoResponse, ApiError> {
    send_all(command::refresh_sends(&target(&name).await?)).await
}

async fn restart(name: DeviceName) -> Result<impl IntoResponse, ApiError> {
    send_all(command::restart_device_sends(&target(&name).await?)).await
}

async fn legacy_mode(name: DeviceName) -> Result<impl IntoResponse, ApiError> {
    send_all(command::legacy_mode_sends(&target(&name).await?)).await
}

#[derive(Deserialize)]
struct Silence {
    on: bool,
}

async fn silence(
    name: DeviceName,
    Json(body): Json<Silence>,
) -> Result<impl IntoResponse, ApiError> {
    send_all(command::silence_sends(&target(&name).await?, body.on)).await
}

#[derive(Deserialize)]
struct RelayBody {
    /// `on`, `off`, `up` or `down`.
    state: String<8>,
    /// Switch back after this many milliseconds; `0` stays.
    #[serde(default)]
    duration_ms: u32,
    #[serde(default)]
    bank: u8,
}

async fn relay_or_rollershutter(
    name: DeviceName,
    number: u8,
    body: RelayBody,
    rollershutter: bool,
) -> Result<impl IntoResponse, ApiError> {
    let state = parse_relay_state(&body.state)
        .ok_or(ApiError::invalid("state must be on, off, up or down"))?;
    if body.duration_ms > 0x00FF_FFFF {
        return Err(ApiError::invalid("duration_ms is limited to 24 bits"));
    }
    let msg = RelaisMsg {
        number,
        state,
        time_ms: body.duration_ms,
        bank: body.bank,
    };
    let sends = command::relais_rollershutter_sends(&target(&name).await?, rollershutter, msg);
    send_all(sends).await
}

async fn relay(
    (name, number): (DeviceName, u8),
    Json(body): Json<RelayBody>,
) -> Result<impl IntoResponse, ApiError> {
    relay_or_rollershutter(name, number, body, false).await
}

async fn rollershutter(
    (name, number): (DeviceName, u8),
    Json(body): Json<RelayBody>,
) -> Result<impl IntoResponse, ApiError> {
    relay_or_rollershutter(name, number, body, true).await
}

#[derive(Deserialize)]
struct LampBody {
    value: u8,
    /// Which of the bank's 24 lamps the value applies to (bit 0 = lamp 1).
    bitmask: u32,
}

async fn lamps(
    (name, bank): (DeviceName, u8),
    Json(body): Json<LampBody>,
) -> Result<impl IntoResponse, ApiError> {
    if body.bitmask > 0x00FF_FFFF {
        return Err(ApiError::invalid("bitmask has 24 bits"));
    }
    let msg = LampMsg {
        value: body.value,
        bitmask: body.bitmask,
        bank,
    };
    // The full form, with the bank (the RPC's `lamp_sends` leaves it out).
    let sends = command::targeted_sends(
        &target(&name).await?,
        CanMessageType::LampGroup,
        &msg.to_bytes_full(),
        false,
    );
    send_all(sends).await
}

// ---- heating controller --------------------------------------------------

#[derive(Deserialize)]
struct HeatingTargetBody {
    flow_c: f32,
    tank: bool,
}

#[derive(Deserialize)]
struct HeatingEnergyBody {
    power_w: i32,
    rod_enabled: bool,
}

#[derive(Deserialize)]
struct HeatingEmergencyBody {
    stage: u8,
    tank_c: f32,
}

async fn heating(
    name: DeviceName,
    msg_type: CanMessageType,
    data: &[u8],
) -> Result<impl IntoResponse, ApiError> {
    let sends = command::targeted_sends(&target(&name).await?, msg_type, data, false);
    send_all(sends).await
}

#[derive(Deserialize)]
struct HeatingManualBody {
    /// Manual mode on (the relays 6-10 go off) or off (the controller has them back).
    enabled: bool,
    /// The relay to switch, 6-10.
    relay: Option<u8>,
    /// Whether that relay goes on.
    #[serde(default)]
    on: bool,
}

async fn heating_manual(
    name: DeviceName,
    Json(body): Json<HeatingManualBody>,
) -> Result<impl IntoResponse, ApiError> {
    if body.relay.is_some_and(|relay| !(6..=10).contains(&relay)) {
        return Err(ApiError::invalid(
            "relay: only 6-10 can be switched by hand",
        ));
    }
    if body.relay.is_some() && !body.enabled {
        return Err(ApiError::invalid("relay: needs manual mode on"));
    }
    let msg = gateway_core::heating_message::HeatingManual {
        enabled: body.enabled,
        relay: body.relay,
        relay_on: body.on,
    };
    heating(name, CanMessageType::HeatingManual, &msg.to_bytes()).await
}

async fn heating_target(
    name: DeviceName,
    Json(body): Json<HeatingTargetBody>,
) -> Result<impl IntoResponse, ApiError> {
    let raw = celsius_to_raw(body.flow_c).ok_or(ApiError::invalid("flow_c must be -50…150"))?;
    let msg = gateway_core::heating_message::HeatingTarget {
        flow_target_raw: raw,
        tank_mode: body.tank,
    };
    heating(name, CanMessageType::HeatingTarget, &msg.to_bytes()).await
}

async fn heating_energy(
    name: DeviceName,
    Json(body): Json<HeatingEnergyBody>,
) -> Result<impl IntoResponse, ApiError> {
    let msg = gateway_core::heating_message::HeatingEnergy {
        power_w: body.power_w,
        rod_enabled: body.rod_enabled,
    };
    heating(name, CanMessageType::HeatingEnergy, &msg.to_bytes()).await
}

async fn heating_emergency(
    name: DeviceName,
    Json(body): Json<HeatingEmergencyBody>,
) -> Result<impl IntoResponse, ApiError> {
    if body.stage > 3 {
        return Err(ApiError::invalid("stage must be 0…3"));
    }
    let raw = celsius_to_raw(body.tank_c).ok_or(ApiError::invalid("tank_c must be -50…150"))?;
    let msg = gateway_core::heating_message::HeatingEmergency {
        stage: body.stage,
        tank_target_raw: raw,
    };
    heating(name, CanMessageType::HeatingEmergency, &msg.to_bytes()).await
}

// ---- device settings -----------------------------------------------------

/// Every field is optional; only the given ones are sent to the device.
#[derive(Deserialize)]
pub struct SaveBody {
    custom_string: Option<String<8>>,
    hwrev: Option<u8>,
    baudrate: Option<u8>,
    extension_mode: Option<String<20>>,
    relais_mode: Option<String<20>>,
    /// Both of `device_id` and `device_type` have to be given together.
    device_id: Option<u8>,
    device_type: Option<u8>,
    /// `0x…` hexadecimal, only used together with `device_id`/`device_type`.
    uid0: Option<String<20>>,
    uid1: Option<String<20>>,
}

pub async fn save(
    name: DeviceName,
    Json(body): Json<SaveBody>,
) -> Result<impl IntoResponse, ApiError> {
    let device_id_type = match (body.device_id, body.device_type) {
        (Some(id), Some(device_type)) => Some((id, device_type)),
        (None, None) => None,
        _ => {
            return Err(ApiError::invalid(
                "device_id and device_type can only be changed together",
            ))
        }
    };
    let hex = |text: &Option<String<20>>| -> Result<Option<u64>, ApiError> {
        match text {
            Some(text) => parse_hex_u64(text)
                .map(Some)
                .ok_or(ApiError::invalid("uid must be 0x… hexadecimal")),
            None => Ok(None),
        }
    };
    let fields = SaveDeviceFields {
        uid0: hex(&body.uid0)?,
        uid1: hex(&body.uid1)?,
        device_id_type,
        baudrate: body.baudrate,
        hwrev: body.hwrev,
        extension_mode: match &body.extension_mode {
            Some(mode) => Some(
                strict_extension_mode(mode).ok_or(ApiError::invalid("unknown extension_mode"))?,
            ),
            None => None,
        },
        custom_string: body.custom_string,
        relais_mode: match &body.relais_mode {
            Some(mode) => {
                Some(strict_relais_mode(mode).ok_or(ApiError::invalid("unknown relais_mode"))?)
            }
            None => None,
        },
    };
    send_all(command::save_device_sends(&target(&name).await?, &fields)).await
}

// ---- every device of one type -------------------------------------------

async fn type_ping(device_type: u8) -> Result<impl IntoResponse, ApiError> {
    send_all(command::ping_sends(&Target::CanByType(device_type))).await
}

async fn type_refresh(device_type: u8) -> Result<impl IntoResponse, ApiError> {
    send_all(command::refresh_sends(&Target::CanByType(device_type))).await
}

async fn type_restart(device_type: u8) -> Result<impl IntoResponse, ApiError> {
    send_all(command::restart_device_sends(&Target::CanByType(
        device_type,
    )))
    .await
}

// ---- everybody on the bus ------------------------------------------------

async fn bus_ping() -> Result<impl IntoResponse, ApiError> {
    send_all(command::ping_sends(&Target::CanAll)).await
}

async fn bus_refresh() -> Result<impl IntoResponse, ApiError> {
    send_all(command::refresh_sends(&Target::CanAll)).await
}

async fn bus_restart() -> Result<impl IntoResponse, ApiError> {
    send_all(command::restart_device_sends(&Target::CanAll)).await
}

async fn bus_silence(Json(body): Json<Silence>) -> Result<impl IntoResponse, ApiError> {
    send_all(command::silence_sends(&Target::CanAll, body.on)).await
}
