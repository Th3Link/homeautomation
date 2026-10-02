//! `canbus/relais_state/#`, `canbus/rollershutter_state/#`,
//! `canbus/relais_command/#`, `canbus/rollershutter_command/#` — mirrors
//! `BridgeRelais.cpp`.

use super::{write_device_segment, Topic};
use crate::helper::mqtt_split;
use crate::relais::MqttCommand;
use crate::DecodeError;
use core::fmt::Write;

pub const RELAIS_COMMAND_TOPIC: &str = "canbus/relais_command/#";
pub const ROLLERSHUTTER_COMMAND_TOPIC: &str = "canbus/rollershutter_command/#";

pub fn is_relais_command_topic(topic: &str) -> bool {
    topic.starts_with("canbus/relais_command/")
}

pub fn is_rollershutter_command_topic(topic: &str) -> bool {
    topic.starts_with("canbus/rollershutter_command/")
}

/// `canbus/relais_state/0x<id>` or `canbus/relais_state/<custom_string>`.
pub fn relais_state_topic(device_key: u32, custom_string: &str) -> Topic {
    let mut t = Topic::new();
    let _ = t.push_str("canbus/relais_state/");
    write_device_segment(&mut t, device_key, custom_string);
    t
}

/// `canbus/rollershutter_state/0x<id>/<bank>/<number>` or
/// `canbus/rollershutter_state/<custom_string>/<bank>/<number>`.
pub fn rollershutter_state_topic(
    device_key: u32,
    custom_string: &str,
    bank: u8,
    number: u8,
) -> Topic {
    let mut t = Topic::new();
    let _ = t.push_str("canbus/rollershutter_state/");
    write_device_segment(&mut t, device_key, custom_string);
    let _ = write!(t, "/{bank}/{number}");
    t
}

/// Parses a `canbus/relais_command/#`/`canbus/rollershutter_command/#`
/// message body: `"<num>/<state>/<stop_time_ms>"`
/// (`BridgeRelais::dispatch`'s topic handler).
pub fn parse_command_body(data: &str) -> Result<MqttCommand, DecodeError> {
    let num = mqtt_split(data, 0).parse().map_err(|_| DecodeError)?;
    let state = mqtt_split(data, 1).parse().map_err(|_| DecodeError)?;
    let stop_time_ms = mqtt_split(data, 2).parse().map_err(|_| DecodeError)?;
    Ok(MqttCommand {
        num,
        state,
        stop_time_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_topic_prefix_matching() {
        assert!(is_relais_command_topic("canbus/relais_command/kitchen"));
        assert!(!is_relais_command_topic(
            "canbus/rollershutter_command/kitchen"
        ));
        assert!(is_rollershutter_command_topic(
            "canbus/rollershutter_command/0x1"
        ));
    }

    #[test]
    fn relais_state_topic_uses_hex_id_without_custom_string() {
        assert_eq!(
            relais_state_topic(0x1005_0100, "").as_str(),
            "canbus/relais_state/0x10050100"
        );
    }

    #[test]
    fn relais_state_topic_prefers_custom_string() {
        assert_eq!(
            relais_state_topic(0x1005_0100, "kitchen").as_str(),
            "canbus/relais_state/kitchen"
        );
    }

    #[test]
    fn rollershutter_state_topic_includes_bank_and_number() {
        assert_eq!(
            rollershutter_state_topic(0x1005_0100, "", 1, 2).as_str(),
            "canbus/rollershutter_state/0x10050100/1/2"
        );
        assert_eq!(
            rollershutter_state_topic(0x1005_0100, "shutter", 1, 2).as_str(),
            "canbus/rollershutter_state/shutter/1/2"
        );
    }

    #[test]
    fn parse_command_body_reads_slash_delimited_fields() {
        let cmd = parse_command_body("3/1/500").unwrap();
        assert_eq!(cmd.num, 3);
        assert_eq!(cmd.state, 1);
        assert_eq!(cmd.stop_time_ms, 500);
    }

    #[test]
    fn parse_command_body_rejects_garbage() {
        assert!(parse_command_body("not/numbers/here").is_err());
        assert!(parse_command_body("1/2").is_err());
    }
}
