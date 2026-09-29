//! `canbus/lamp_command/#`, `canbus/nightlight_command/#` — mirrors
//! `BridgeLamps.cpp`. Fire-and-forget: there's no MQTT state feedback for
//! either (the original's CAN dispatcher for this bridge is commented out
//! — `//m_can.add_dispatcher(this);` — "there is no feed back").

use crate::helper::mqtt_split;
use crate::lamp::LampMsg;
use crate::DecodeError;

pub const LAMP_COMMAND_TOPIC: &str = "canbus/lamp_command/#";
pub const NIGHTLIGHT_COMMAND_TOPIC: &str = "canbus/nightlight_command/#";

pub fn is_lamp_command_topic(topic: &str) -> bool {
    topic.starts_with("canbus/lamp_command/")
}

pub fn is_nightlight_command_topic(topic: &str) -> bool {
    topic.starts_with("canbus/nightlight_command/")
}

/// Parses a `canbus/lamp_command/#` body: `"<value(decimal)>/<bitmask(hex)>[/<bank(decimal)>]"`.
/// Returns the decoded [`LampMsg`] plus whether a `bank` segment was
/// present — the caller must use [`LampMsg::to_bytes_full`] when it was
/// (`true`) and [`LampMsg::to_bytes_short`] when it wasn't (`false`),
/// matching `BridgeLamps::dispatch`'s conditional 4-vs-8-byte send.
pub fn parse_lamp_command_body(data: &str) -> Result<(LampMsg, bool), DecodeError> {
    let value: u8 = mqtt_split(data, 0).parse().map_err(|_| DecodeError)?;
    let bitmask = u32::from_str_radix(mqtt_split(data, 1), 16).map_err(|_| DecodeError)?;
    let bank_field = mqtt_split(data, 2);
    let has_bank = !bank_field.is_empty();
    let bank = if has_bank {
        bank_field.parse().map_err(|_| DecodeError)?
    } else {
        0
    };
    Ok((
        LampMsg {
            value,
            bitmask,
            bank,
        },
        has_bank,
    ))
}

/// Parses a `canbus/nightlight_command/#` body: a plain decimal integer,
/// the whole payload (not slash-delimited).
pub fn parse_nightlight_command_body(data: &str) -> Result<u8, DecodeError> {
    data.parse().map_err(|_| DecodeError)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lamp_topic_prefix_matching() {
        assert!(is_lamp_command_topic("canbus/lamp_command/kitchen"));
        assert!(is_nightlight_command_topic(
            "canbus/nightlight_command/kitchen"
        ));
        assert!(!is_lamp_command_topic("canbus/nightlight_command/kitchen"));
    }

    #[test]
    fn parse_lamp_command_without_bank_uses_short_form() {
        let (msg, has_bank) = parse_lamp_command_body("255/ff").unwrap();
        assert_eq!(msg.value, 255);
        assert_eq!(msg.bitmask, 0xFF);
        assert_eq!(msg.bank, 0);
        assert!(!has_bank);
    }

    #[test]
    fn parse_lamp_command_with_bank_uses_full_form() {
        let (msg, has_bank) = parse_lamp_command_body("1/a0/3").unwrap();
        assert_eq!(msg.bank, 3);
        assert!(has_bank);
    }

    #[test]
    fn parse_lamp_command_bitmask_is_hex_value_is_decimal() {
        // Regression test: value uses base 10, bitmask uses base 16 —
        // swapping them would silently misparse.
        let (msg, _) = parse_lamp_command_body("10/10").unwrap();
        assert_eq!(msg.value, 10);
        assert_eq!(msg.bitmask, 0x10);
    }

    #[test]
    fn parse_nightlight_command_is_a_plain_integer() {
        assert_eq!(parse_nightlight_command_body("5"), Ok(5));
        assert!(parse_nightlight_command_body("5/6").is_err());
    }
}
