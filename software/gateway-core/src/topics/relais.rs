//! `canbus/relais_state/#`, `canbus/rollershutter_state/#`,
//! `canbus/relais_command/#`, `canbus/rollershutter_command/#` — mirrors
//! `BridgeRelais.cpp`.

use super::{write_device_segment, Topic};
use crate::helper::mqtt_split;
use crate::relais::{MqttCommand, RelaisMsg};
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

/// `canbus/relais_state/0x<id>/<bank>/<number>` or
/// `canbus/relais_state/<custom_string>/<bank>/<number>`: one relay
/// channel's state, unlike [`relais_state_topic`], which carries only the
/// channel number.
pub fn relais_channel_state_topic(
    device_key: u32,
    custom_string: &str,
    bank: u8,
    number: u8,
) -> Topic {
    let mut t = relais_state_topic(device_key, custom_string);
    let _ = write!(t, "/{bank}/{number}");
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

/// The CAN frame for a `canbus/relais_command/#`/
/// `canbus/rollershutter_command/#` body: `"<num>/<state>/<stop_time_ms>"`
/// as the original gateway sent it ([`MqttCommand`]), or with a fourth
/// field, `"<num>/<state>/<time_ms>/<bank>"`, for the relay boards of a
/// node other than its own (bank 1-4 on the extension bus), in the
/// [`RelaisMsg`] layout. With a bank the time is limited to 24 bits, as the
/// bank takes the top byte.
pub fn command_frame(data: &str) -> Result<[u8; 8], DecodeError> {
    let cmd = parse_command_body(data)?;
    let bank = mqtt_split(data, 3);
    if bank.is_empty() {
        return Ok(cmd.to_bytes());
    }
    let bank = bank.parse().map_err(|_| DecodeError)?;
    if cmd.stop_time_ms > 0x00FF_FFFF {
        return Err(DecodeError);
    }
    Ok(RelaisMsg {
        number: cmd.num,
        state: cmd.state,
        time_ms: cmd.stop_time_ms,
        bank,
    }
    .to_bytes())
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
    fn relais_channel_state_topic_includes_bank_and_number() {
        assert_eq!(
            relais_channel_state_topic(0x1005_8000, "", 0, 9).as_str(),
            "canbus/relais_state/0x10058000/0/9"
        );
        assert_eq!(
            relais_channel_state_topic(0x1005_8000, "basement", 0, 9).as_str(),
            "canbus/relais_state/basement/0/9"
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
    fn command_frame_without_bank_is_the_original_one() {
        assert_eq!(
            command_frame("3/1/500").unwrap(),
            [3, 1, 0xF4, 0x01, 0, 0, 0, 0]
        );
        // The top byte of a long time lands where a node reads the bank.
        assert_eq!(command_frame("3/3/16777216").unwrap()[5], 1);
    }

    #[test]
    fn command_frame_carries_the_bank() {
        assert_eq!(
            command_frame("11/3/500/2").unwrap(),
            [11, 3, 0xF4, 0x01, 0, 2, 0, 0]
        );
        assert_eq!(command_frame("0/0/0/4").unwrap(), [0, 0, 0, 0, 0, 4, 0, 0]);
        assert!(command_frame("1/3/16777216/1").is_err());
        assert!(command_frame("1/3/0/x").is_err());
        assert!(command_frame("1/3/0/256").is_err());
    }

    #[test]
    fn parse_command_body_rejects_garbage() {
        assert!(parse_command_body("not/numbers/here").is_err());
        assert!(parse_command_body("1/2").is_err());
    }
}
