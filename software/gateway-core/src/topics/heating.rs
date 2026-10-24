//! `canbus/heating_target_command/#`, `canbus/heating_energy_command/#`,
//! `canbus/heating_emergency_command/#` — the commands for a
//! `HeatingControl` node. These are new (the original C++ gateway has no
//! heating controller), so unlike the other bridges there is nothing to
//! mirror; they follow the shape of the existing command topics: the device
//! in the topic's 3rd segment, a `/`-separated body.
//!
//! The node treats each of these as missing 30 s after it last arrived (and
//! then switches the heating rod off / returns the circuit to direct
//! operation), so whoever publishes them has to keep repeating them — about
//! every 5 s — and **must not publish them retained**: the gateway ignores
//! retained deliveries on these topics (see
//! `gateway-hardware::translate::dispatch_mqtt`), because replaying an old
//! value after a reconnect would look like a fresh one to the node.

use crate::helper::mqtt_split;
use crate::DecodeError;
use common_core::heating_message::{
    millicelsius_to_raw, HeatingEmergency, HeatingEnergy, HeatingTarget,
};

pub const HEATING_TARGET_COMMAND_TOPIC: &str = "canbus/heating_target_command/#";
pub const HEATING_ENERGY_COMMAND_TOPIC: &str = "canbus/heating_energy_command/#";
pub const HEATING_EMERGENCY_COMMAND_TOPIC: &str = "canbus/heating_emergency_command/#";

pub fn is_heating_target_command_topic(topic: &str) -> bool {
    topic.starts_with("canbus/heating_target_command/")
}

pub fn is_heating_energy_command_topic(topic: &str) -> bool {
    topic.starts_with("canbus/heating_energy_command/")
}

pub fn is_heating_emergency_command_topic(topic: &str) -> bool {
    topic.starts_with("canbus/heating_emergency_command/")
}

/// Temperatures outside this range (in °C) are rejected as typos.
const MIN_CELSIUS: i32 = -50;
const MAX_CELSIUS: i32 = 150;

/// Parses a decimal temperature such as `45`, `45.5` or `-3.25` into
/// milli-degrees Celsius. Digits after the third decimal place are
/// ignored. No exponents, no thousands separators.
fn parse_millicelsius(text: &str) -> Option<i32> {
    let text = text.trim();
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    if whole.is_empty() && fraction.is_empty() {
        return None;
    }
    if !whole.bytes().all(|b| b.is_ascii_digit()) || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let whole: i32 = if whole.is_empty() {
        0
    } else {
        whole.parse().ok()?
    };
    let mut milli = 0;
    for (i, digit) in fraction.bytes().take(3).enumerate() {
        milli += i32::from(digit - b'0') * [100, 10, 1][i];
    }
    let value = whole.checked_mul(1000)?.checked_add(milli)?;
    Some(if negative { -value } else { value })
}

/// A temperature in °C as the node's 1/16 °C wire value.
fn parse_celsius_raw(text: &str) -> Result<i16, DecodeError> {
    let milli = parse_millicelsius(text).ok_or(DecodeError)?;
    if !(MIN_CELSIUS * 1000..=MAX_CELSIUS * 1000).contains(&milli) {
        return Err(DecodeError);
    }
    Ok(millicelsius_to_raw(milli))
}

/// `0`/`1`, or `false`/`true`, or `off`/`on`.
fn parse_flag(text: &str) -> Result<bool, DecodeError> {
    match text.trim() {
        "1" | "true" | "on" => Ok(true),
        "0" | "false" | "off" => Ok(false),
        _ => Err(DecodeError),
    }
}

/// Body of `canbus/heating_target_command/<device>`:
/// `<flow_target_°C>/<tank_mode>` — e.g. `45.5/1` asks for a 45.5 °C flow,
/// supplied from the tank.
pub fn parse_target_body(data: &str) -> Result<HeatingTarget, DecodeError> {
    if !mqtt_split(data, 2).is_empty() {
        return Err(DecodeError);
    }
    Ok(HeatingTarget {
        flow_target_raw: parse_celsius_raw(mqtt_split(data, 0))?,
        tank_mode: parse_flag(mqtt_split(data, 1))?,
    })
}

/// Body of `canbus/heating_energy_command/<device>`:
/// `<power_W>/<rod_enabled>` — e.g. `6200/1` for 6.2 kW of surplus with the
/// heating rod released; negative power while importing.
pub fn parse_energy_body(data: &str) -> Result<HeatingEnergy, DecodeError> {
    if !mqtt_split(data, 2).is_empty() {
        return Err(DecodeError);
    }
    Ok(HeatingEnergy {
        power_w: mqtt_split(data, 0)
            .trim()
            .parse()
            .map_err(|_| DecodeError)?,
        rod_enabled: parse_flag(mqtt_split(data, 1))?,
    })
}

/// Body of `canbus/heating_emergency_command/<device>`:
/// `<stage>/<tank_target_°C>` — e.g. `2/60` runs rod stage 2 until the tank
/// is at 60 °C. Stage 0 means "no emergency".
pub fn parse_emergency_body(data: &str) -> Result<HeatingEmergency, DecodeError> {
    if !mqtt_split(data, 2).is_empty() {
        return Err(DecodeError);
    }
    let stage: u8 = mqtt_split(data, 0)
        .trim()
        .parse()
        .map_err(|_| DecodeError)?;
    if stage > 3 {
        return Err(DecodeError);
    }
    Ok(HeatingEmergency {
        stage,
        tank_target_raw: parse_celsius_raw(mqtt_split(data, 1))?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topic_prefix_matching() {
        assert!(is_heating_target_command_topic(
            "canbus/heating_target_command/HEIZUNG1"
        ));
        assert!(!is_heating_target_command_topic(
            "canbus/heating_energy_command/HEIZUNG1"
        ));
        assert!(is_heating_energy_command_topic(
            "canbus/heating_energy_command/0x9"
        ));
        assert!(is_heating_emergency_command_topic(
            "canbus/heating_emergency_command/0x9"
        ));
        assert!(!is_heating_emergency_command_topic(
            "canbus/relais_command/0x9"
        ));
    }

    #[test]
    fn decimals_parse_to_milli_degrees() {
        assert_eq!(parse_millicelsius("45"), Some(45_000));
        assert_eq!(parse_millicelsius("45.5"), Some(45_500));
        assert_eq!(parse_millicelsius("-3.25"), Some(-3_250));
        assert_eq!(parse_millicelsius("+7"), Some(7_000));
        assert_eq!(parse_millicelsius(".5"), Some(500));
        assert_eq!(parse_millicelsius("20.12345"), Some(20_123));
        assert_eq!(parse_millicelsius(" 21 "), Some(21_000));
    }

    #[test]
    fn malformed_decimals_are_rejected() {
        for text in [
            "", ".", "-", "abc", "1e3", "1,5", "4 5", "--3", "1.2.3", "0x10",
        ] {
            assert_eq!(parse_millicelsius(text), None, "{text:?}");
        }
    }

    #[test]
    fn target_body_encodes_to_the_wire_message() {
        let msg = parse_target_body("45.5/1").unwrap();
        assert_eq!(msg.to_bytes(), [0xD8, 0x02, 0x01]);
        let msg = parse_target_body("40/0").unwrap();
        assert!(!msg.tank_mode);
        assert_eq!(msg.flow_target_millicelsius(), 40_000);
    }

    #[test]
    fn flags_accept_the_usual_spellings() {
        for (text, expected) in [
            ("1", true),
            ("true", true),
            ("on", true),
            ("0", false),
            ("false", false),
            ("off", false),
        ] {
            assert_eq!(
                parse_target_body(&format_flag(text)).unwrap().tank_mode,
                expected
            );
        }
        assert!(parse_target_body("40/yes").is_err());
        assert!(parse_target_body("40/2").is_err());
    }

    fn format_flag(flag: &str) -> heapless::String<16> {
        let mut body = heapless::String::new();
        body.push_str("40/").unwrap();
        body.push_str(flag).unwrap();
        body
    }

    #[test]
    fn target_body_rejects_garbage_and_out_of_range_values() {
        assert!(parse_target_body("").is_err());
        assert!(parse_target_body("45").is_err());
        assert!(parse_target_body("45/1/9").is_err());
        assert!(parse_target_body("hot/1").is_err());
        assert!(parse_target_body("151/1").is_err());
        assert!(parse_target_body("-51/1").is_err());
        assert!(parse_target_body("150/1").is_ok());
    }

    #[test]
    fn energy_body_reads_watts_and_release() {
        let msg = parse_energy_body("6200/1").unwrap();
        assert_eq!(msg.power_w, 6_200);
        assert!(msg.rod_enabled);
        assert_eq!(msg.to_bytes(), [0x38, 0x18, 0, 0, 1]);
        let msg = parse_energy_body("-1500/0").unwrap();
        assert_eq!(msg.power_w, -1_500);
        assert!(!msg.rod_enabled);
    }

    #[test]
    fn energy_body_rejects_garbage() {
        assert!(parse_energy_body("").is_err());
        assert!(parse_energy_body("6200").is_err());
        assert!(parse_energy_body("6.2/1").is_err());
        assert!(parse_energy_body("many/1").is_err());
        assert!(parse_energy_body("6200/1/x").is_err());
    }

    #[test]
    fn emergency_body_reads_stage_and_tank_target() {
        let msg = parse_emergency_body("2/60").unwrap();
        assert_eq!(msg.stage, 2);
        assert_eq!(msg.tank_target_millicelsius(), 60_000);
        assert_eq!(msg.to_bytes(), [2, 0xC0, 0x03]);
        assert_eq!(parse_emergency_body("0/60").unwrap().stage, 0);
    }

    #[test]
    fn emergency_body_rejects_garbage_and_invalid_stages() {
        assert!(parse_emergency_body("4/60").is_err());
        assert!(parse_emergency_body("-1/60").is_err());
        assert!(parse_emergency_body("2").is_err());
        assert!(parse_emergency_body("2/hot").is_err());
        assert!(parse_emergency_body("2/60/1").is_err());
    }
}
