//! CAN<->MQTT translation wiring — the concrete counterpart to
//! [`gateway_core::topics`], connecting [`crate::can`]'s receive dispatch
//! to [`crate::mqtt`] and back. One function per original `Bridge*.cpp`.

use crate::device_list::DEVICE_LIST;
use gateway_core::button::ButtonEvent;
use gateway_core::can_id::CanId;
use gateway_core::can_message_type::CanMessageType;
use gateway_core::device_message::encode_id_type;
use gateway_core::relais::{self, RelaisMsg};
use gateway_core::sensor::{decode_ambient_light, FixedPointReading};
use gateway_core::topics::{button, debug, device, lamp, relais as relais_topics};
use heapless::String as HString;

/// Routes one received CAN frame to whichever handler cares about its
/// `msg_type`. Called from [`crate::can::dispatch`] after the device-list
/// bookkeeping every frame gets regardless of type.
pub async fn dispatch(id: CanId, data: &[u8], _request: bool) {
    crate::echo::dispatch(id, data).await;

    match id.msg_type {
        CanMessageType::RelaisState => {
            if let Some(state) = relais::decode_relais_state(data) {
                let mut body: HString<8> = HString::new();
                let _ = core::fmt::write(&mut body, format_args!("{state}"));
                publish_with_custom_string(
                    id.device_key(),
                    |cs| relais_topics::relais_state_topic(id.device_key(), cs),
                    body.as_str(),
                )
                .await;
            }
        }
        CanMessageType::RollershutterState => {
            if let Ok(msg) = RelaisMsg::from_bytes(data) {
                let mut body: HString<8> = HString::new();
                let _ = core::fmt::write(&mut body, format_args!("{}", msg.state));
                let key = id.device_key();
                publish_with_custom_string(
                    key,
                    |cs| relais_topics::rollershutter_state_topic(key, cs, msg.bank, msg.number),
                    body.as_str(),
                )
                .await;
            }
        }
        CanMessageType::ButtonEvent => {
            if let Ok(event) = ButtonEvent::from_bytes(data) {
                if let Some(body) = button::button_body(event) {
                    let key = id.device_key();
                    publish_with_custom_string(
                        key,
                        |cs| button::button_topic(key, cs),
                        body.as_str(),
                    )
                    .await;
                }
            }
        }
        CanMessageType::PirSensor => {
            if let Ok(event) = ButtonEvent::from_bytes(data) {
                let body = button::presence_body(event);
                let key = id.device_key();
                publish_with_custom_string(
                    key,
                    |cs| button::presence_topic(key, cs),
                    body.as_str(),
                )
                .await;
            }
        }
        CanMessageType::TemperatureSensor
        | CanMessageType::PressureSensor
        | CanMessageType::HumiditySensor
        | CanMessageType::Co2Equivalent
        | CanMessageType::VocBreath
        | CanMessageType::AirQuality => {
            if let Ok(reading) = FixedPointReading::from_bytes(data) {
                let mut body: HString<24> = HString::new();
                let _ = core::fmt::write(&mut body, format_args!("{}", reading.value()));
                let key = id.device_key();
                let msg_type = id.msg_type;
                publish_with_custom_string(
                    key,
                    |cs| device::fixed_point_sensor_topic(msg_type, key, cs, reading.sub_id),
                    body.as_str(),
                )
                .await;
            }
        }
        CanMessageType::AmbientLightSensor => {
            if let Ok(value) = decode_ambient_light(data) {
                let mut body: HString<16> = HString::new();
                let _ = core::fmt::write(&mut body, format_args!("{value}"));
                let key = id.device_key();
                publish_with_custom_string(
                    key,
                    |cs| device::ambient_light_topic(key, cs),
                    body.as_str(),
                )
                .await;
            }
        }
        // `Available` frames from other devices aren't actually republished
        // by the original either (see gateway_core::topics::device's doc
        // comment) — only this gateway's own self-announce on MQTT connect
        // is, which is handled separately at connect time.
        _ => {}
    }
}

/// Looks up `key`'s custom string, builds a topic with it via `topic_fn`,
/// and publishes `body` to it.
async fn publish_with_custom_string(
    key: u32,
    topic_fn: impl FnOnce(&str) -> gateway_core::topics::Topic,
    body: &str,
) {
    let custom_string = {
        let table = DEVICE_LIST.lock().await;
        let mut cs: HString<10> = HString::new();
        let _ = cs.push_str(table.entry(key));
        cs
    };
    let topic = topic_fn(custom_string.as_str());
    crate::mqtt::publish(topic.as_str(), body).await;
}

/// The gateway's own self-announce, sent once on every MQTT (re)connect
/// (`BridgeDevice::connected_event`).
pub async fn publish_self_available() {
    let device_key = CanId::new(
        crate::can::DEVICE_TYPE.load(core::sync::atomic::Ordering::Relaxed),
        crate::can::DEVICE_ID.load(core::sync::atomic::Ordering::Relaxed),
        CanMessageType::Available,
    )
    .device_key();
    let topic = device::available_topic(device_key);
    crate::mqtt::publish(topic.as_str(), device::AVAILABLE_BODY).await;
}

/// Routes one received MQTT message to whichever bridge's command topic it
/// matches. Called from [`crate::mqtt`]'s session loop.
pub async fn dispatch_mqtt(topic: &str, body: &str) {
    if gateway_core::topics::is_gateway_restart_topic(topic) {
        esp_hal::system::software_reset();
    }

    if relais_topics::is_relais_command_topic(topic)
        || relais_topics::is_rollershutter_command_topic(topic)
    {
        let Ok(cmd) = relais_topics::parse_command_body(body) else {
            return;
        };
        let Some(target) = resolve_target(topic).await else {
            return;
        };
        let msg_type = if relais_topics::is_rollershutter_command_topic(topic) {
            CanMessageType::Rollershutter
        } else {
            CanMessageType::Relais
        };
        send_command(target, msg_type, &cmd.to_bytes()).await;
        return;
    }

    if lamp::is_lamp_command_topic(topic) {
        let Ok((msg, has_bank)) = lamp::parse_lamp_command_body(body) else {
            return;
        };
        let Some(target) = resolve_target(topic).await else {
            return;
        };
        if has_bank {
            send_command(target, CanMessageType::LampGroup, &msg.to_bytes_full()).await;
        } else {
            send_command(target, CanMessageType::LampGroup, &msg.to_bytes_short()).await;
        }
        return;
    }

    if lamp::is_nightlight_command_topic(topic) {
        let Ok(value) = lamp::parse_nightlight_command_body(body) else {
            return;
        };
        let Some(target) = resolve_target(topic).await else {
            return;
        };
        send_command(
            target,
            CanMessageType::Nightlight,
            &gateway_core::lamp::nightlight_bytes(value),
        )
        .await;
        return;
    }

    if debug::is_debug_topic(topic) {
        if let Ok((target_id, bytes)) = debug::parse_debug_command(topic, body) {
            crate::can::send_raw(target_id, &bytes, false).await;
        }
    }
}

async fn resolve_target(topic: &str) -> Option<u32> {
    let device_string = gateway_core::topics::command_target(topic);
    let table = DEVICE_LIST.lock().await;
    table.resolve(device_string)
}

async fn send_command(target_key: u32, msg_type: CanMessageType, data: &[u8]) {
    let id = target_key + u8::from(msg_type) as u32;
    crate::can::send_raw(id, data, false).await;
}

/// `Command::save_device`'s `DeviceIdType`/`DeviceUid0`/`DeviceUid1` triple
/// — exposed for the web config server's (not yet implemented — see
/// `docs/technical-debt.md`) `/control.json` handler.
pub async fn send_device_id_type(target_key: u32, device_id: u8, device_type: u8) {
    send_command(
        target_key,
        CanMessageType::DeviceIdType,
        &encode_id_type(device_id, device_type),
    )
    .await;
}
