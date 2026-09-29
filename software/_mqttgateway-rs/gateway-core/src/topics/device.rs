//! `canbus/<sensor>/#`, `canbus/available/#` — mirrors `BridgeDevice.cpp`.
//! CAN->MQTT only.

use super::{write_device_segment, Topic};
use crate::can_message_type::CanMessageType;
use crate::helper::to_hex_string;
use core::fmt::Write;

/// `BridgeDevice::sensor_name`. Message types not covered here (including
/// `AmbientLightSensorWhite`, which the original's `dispatch` doesn't
/// check for at all) render as `"unknown"` — `dispatch_sensor` is simply
/// never called for them, so `"unknown"` topics never actually get built
/// in practice, but the mapping itself is total.
pub fn sensor_name(msg_type: CanMessageType) -> &'static str {
    match msg_type {
        CanMessageType::TemperatureSensor => "temperature",
        CanMessageType::PressureSensor => "pressure",
        CanMessageType::HumiditySensor => "humidity",
        CanMessageType::Co2Equivalent => "co2",
        CanMessageType::VocBreath => "voc",
        CanMessageType::AirQuality => "air_quality",
        CanMessageType::AmbientLightSensor => "brightness",
        _ => "unknown",
    }
}

/// Topic for a fixed-point sensor reading (temperature/pressure/humidity/
/// co2/voc/air_quality): `canbus/<name>/0x<device>/0x<sub_id>` or
/// `canbus/<name>/<custom_string>/0x<sub_id>`.
///
/// `sub_id` is the reading's accompanying 48-bit id
/// ([`crate::sensor::FixedPointReading::sub_id`]) — but the hex suffix is
/// built from only its **low 32 bits**, matching the original's
/// `toHexString(unsigned int)` silently truncating the wider
/// `uint64_t id:48` bitfield value passed to it. Reproduced as-is: a
/// 48-bit sub-id with any of its top 16 bits set would produce a
/// different-looking topic here than a "fixed" 48-bit-aware formatter
/// would, and real deployed subscribers key off the truncated form.
pub fn fixed_point_sensor_topic(
    msg_type: CanMessageType,
    device_key: u32,
    custom_string: &str,
    sub_id: u64,
) -> Topic {
    let mut t = Topic::new();
    let _ = write!(t, "canbus/{}/", sensor_name(msg_type));
    write_device_segment(&mut t, device_key, custom_string);
    let _ = write!(t, "/0x{}", to_hex_string(sub_id as u32));
    t
}

/// Topic for an ambient-light reading: `canbus/brightness/0x<device>` or
/// `canbus/brightness/<custom_string>` — no sub-id suffix.
pub fn ambient_light_topic(device_key: u32, custom_string: &str) -> Topic {
    let mut t = Topic::new();
    let _ = t.push_str("canbus/brightness/");
    write_device_segment(&mut t, device_key, custom_string);
    t
}

/// `canbus/available/<hex>` — self-announce on MQTT (re)connect
/// (`BridgeDevice::connected_event`). `device_key` already has
/// `msg_type` zeroed, which is exactly what `Available`'s discriminant
/// (`0`) contributes anyway, so this is the full identifier hex with no
/// further masking needed.
pub fn available_topic(device_key: u32) -> Topic {
    let mut t = Topic::new();
    let _ = write!(t, "canbus/available/{}", to_hex_string(device_key));
    t
}

/// Body published to [`available_topic`] — always this literal string.
pub const AVAILABLE_BODY: &str = "0x01";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensor_name_mapping() {
        assert_eq!(
            sensor_name(CanMessageType::TemperatureSensor),
            "temperature"
        );
        assert_eq!(sensor_name(CanMessageType::AirQuality), "air_quality");
        assert_eq!(
            sensor_name(CanMessageType::AmbientLightSensorWhite),
            "unknown"
        );
    }

    #[test]
    fn fixed_point_topic_shape() {
        let t = fixed_point_sensor_topic(
            CanMessageType::TemperatureSensor,
            0x1005_0100,
            "",
            0x0000_1234,
        );
        assert_eq!(t.as_str(), "canbus/temperature/0x10050100/0x1234");
    }

    #[test]
    fn fixed_point_topic_prefers_custom_string() {
        let t = fixed_point_sensor_topic(
            CanMessageType::HumiditySensor,
            0x1005_0100,
            "bathroom",
            0x1234,
        );
        assert_eq!(t.as_str(), "canbus/humidity/bathroom/0x1234");
    }

    #[test]
    fn fixed_point_topic_sub_id_hex_truncates_to_32_bits() {
        // Regression test for the C++ toHexString(unsigned int) implicit
        // narrowing of the 48-bit sub_id.
        let sub_id_48bit = 0x0001_0000_1234u64; // top 16 bits set
        let t = fixed_point_sensor_topic(
            CanMessageType::TemperatureSensor,
            0x1005_0100,
            "",
            sub_id_48bit,
        );
        assert_eq!(t.as_str(), "canbus/temperature/0x10050100/0x1234");
    }

    #[test]
    fn ambient_light_topic_has_no_suffix() {
        assert_eq!(
            ambient_light_topic(0x1005_0100, "").as_str(),
            "canbus/brightness/0x10050100"
        );
    }

    #[test]
    fn available_topic_and_body() {
        assert_eq!(
            available_topic(0x1006_0100).as_str(),
            "canbus/available/10060100"
        );
        assert_eq!(AVAILABLE_BODY, "0x01");
    }
}
