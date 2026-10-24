//! The hardware-independent parts of the web server's request handling
//! (`gateway-hardware::web`): HTTP Basic Auth decoding and checking, and
//! the values the REST API takes from paths and queries.

use crate::command::{ExtensionMode, RelaisMode};
use heapless::String;

/// Longest user name / password Basic Auth is checked for, same as the
/// configuration's `web_username`/`web_password` fields.
pub const CREDENTIAL_CAP: usize = 30;

/// Decodes the value of an `Authorization: Basic …` header into the user
/// name and password. `None` for any other scheme, malformed Base64, a
/// missing `:` or credentials longer than [`CREDENTIAL_CAP`].
pub fn decode_basic_auth(header: &str) -> Option<(String<CREDENTIAL_CAP>, String<CREDENTIAL_CAP>)> {
    let encoded = header.trim().strip_prefix("Basic ")?.trim();
    let mut decoded = [0u8; 2 * CREDENTIAL_CAP + 1];
    let len = base64_decode(encoded, &mut decoded)?;
    let text = core::str::from_utf8(&decoded[..len]).ok()?;
    let (user, password) = text.split_once(':')?;
    Some((
        String::try_from(user).ok()?,
        String::try_from(password).ok()?,
    ))
}

/// Whether `given` matches the configured credentials. Compares in time
/// independent of where the first difference is, and checks both fields
/// even if the first one already failed.
pub fn credentials_match(
    given: &(String<CREDENTIAL_CAP>, String<CREDENTIAL_CAP>),
    username: &str,
    password: &str,
) -> bool {
    let user_ok = constant_time_eq(given.0.as_bytes(), username.as_bytes());
    let password_ok = constant_time_eq(given.1.as_bytes(), password.as_bytes());
    user_ok & password_ok
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let mut difference = (a.len() ^ b.len()) as u8;
    for i in 0..a.len().max(b.len()) {
        difference |= a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0);
    }
    difference == 0
}

/// Standard Base64 (with or without padding) into `out`; returns the
/// decoded length, or `None` for invalid input or if `out` is too small.
fn base64_decode(input: &str, out: &mut [u8]) -> Option<usize> {
    let input = input.trim_end_matches('=').as_bytes();
    let mut accumulator = 0u32;
    let mut bits = 0u32;
    let mut len = 0;
    for &c in input {
        let value = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        accumulator = (accumulator << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            *out.get_mut(len)? = (accumulator >> bits) as u8;
            len += 1;
            accumulator &= (1 << bits) - 1;
        }
    }
    Some(len)
}

/// `off`/`up`/`down`/`on` (any case) as the relay state byte the nodes
/// understand (`cancomponents_core::relais::State`).
pub fn parse_relay_state(name: &str) -> Option<u8> {
    const STATES: [(&str, u8); 4] = [("off", 0), ("up", 1), ("down", 2), ("on", 3)];
    STATES
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|&(_, value)| value)
}

/// A temperature in °C as the nodes' 1/16 °C wire value; `None` outside
/// -50…150 °C (a typo is more likely than a real value) or for NaN.
pub fn celsius_to_raw(celsius: f32) -> Option<i16> {
    if !(-50.0..=150.0).contains(&celsius) {
        return None;
    }
    Some(common_core::heating_message::millicelsius_to_raw(
        (celsius * 1000.0) as i32,
    ))
}

/// The extension mode called `name` (any case), without
/// [`ExtensionMode::from_name`]'s fallback to `Off` for names it doesn't know.
pub fn strict_extension_mode(name: &str) -> Option<ExtensionMode> {
    const NAMES: [&str; 8] = [
        "OFF",
        "BUTTONS",
        "RELAIS",
        "SWROLLERSHUTTER",
        "HWROLLERSHUTTER",
        "PWM",
        "SENSORS",
        "LEGACY_SENSORS",
    ];
    let upper = upper_name(name)?;
    NAMES
        .contains(&upper.as_str())
        .then(|| ExtensionMode::from_name(&upper))
}

/// The relay mode called `name` (any case), see [`strict_extension_mode`].
pub fn strict_relais_mode(name: &str) -> Option<RelaisMode> {
    const NAMES: [&str; 4] = ["OFF", "RELAIS", "SWROLLERSHUTTER", "HWROLLERSHUTTER"];
    let upper = upper_name(name)?;
    NAMES
        .contains(&upper.as_str())
        .then(|| RelaisMode::from_name(&upper))
}

fn upper_name(name: &str) -> Option<String<20>> {
    let mut out = String::new();
    for c in name.chars() {
        out.push(c.to_ascii_uppercase()).ok()?;
    }
    Some(out)
}

/// A `0x…` hexadecimal number, the way the API shows UIDs.
pub fn parse_hex_u64(text: &str) -> Option<u64> {
    u64::from_str_radix(text.strip_prefix("0x")?, 16).ok()
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    #[test]
    fn decodes_the_default_credentials() {
        // base64("CAN2MQTTSETUP:Can2MqttPass")
        let (user, password) =
            decode_basic_auth("Basic Q0FOMk1RVFRTRVRVUDpDYW4yTXF0dFBhc3M=").unwrap();
        assert_eq!(user.as_str(), "CAN2MQTTSETUP");
        assert_eq!(password.as_str(), "Can2MqttPass");
    }

    #[test]
    fn decodes_without_padding_and_with_empty_password() {
        // base64("a:") = "YTo="
        let (user, password) = decode_basic_auth("Basic YTo").unwrap();
        assert_eq!(user.as_str(), "a");
        assert_eq!(password.as_str(), "");
    }

    #[test]
    fn password_may_contain_colons() {
        // base64("u:p:q")
        let (user, password) = decode_basic_auth("Basic dTpwOnE=").unwrap();
        assert_eq!((user.as_str(), password.as_str()), ("u", "p:q"));
    }

    #[test]
    fn rejects_other_schemes_and_garbage() {
        assert!(decode_basic_auth("Bearer abc").is_none());
        assert!(decode_basic_auth("Basic !!!").is_none());
        // base64("nocolon")
        assert!(decode_basic_auth("Basic bm9jb2xvbg==").is_none());
        assert!(decode_basic_auth("").is_none());
    }

    #[test]
    fn rejects_too_long_credentials() {
        // 31 x 'a' then ":b" - user longer than the cap
        let mut text = std::string::String::new();
        for _ in 0..31 {
            text.push('a');
        }
        text.push_str(":b");
        let encoded = encode(text.as_bytes());
        assert!(decode_basic_auth(&std::format!("Basic {encoded}")).is_none());
    }

    #[test]
    fn matches_only_equal_credentials() {
        let given = decode_basic_auth("Basic dTpw").unwrap(); // u:p
        assert!(credentials_match(&given, "u", "p"));
        assert!(!credentials_match(&given, "u", "q"));
        assert!(!credentials_match(&given, "v", "p"));
        assert!(!credentials_match(&given, "u", "pp"));
        assert!(!credentials_match(&given, "", ""));
    }

    #[test]
    fn relay_states_by_name() {
        assert_eq!(parse_relay_state("off"), Some(0));
        assert_eq!(parse_relay_state("Up"), Some(1));
        assert_eq!(parse_relay_state("DOWN"), Some(2));
        assert_eq!(parse_relay_state("on"), Some(3));
        assert_eq!(parse_relay_state("toggle"), None);
        assert_eq!(parse_relay_state(""), None);
    }

    #[test]
    fn celsius_converts_to_sixteenths() {
        assert_eq!(celsius_to_raw(45.0), Some(720));
        assert_eq!(celsius_to_raw(45.5), Some(728));
        assert_eq!(celsius_to_raw(-3.25), Some(-52));
        assert_eq!(celsius_to_raw(0.0), Some(0));
    }

    #[test]
    fn celsius_out_of_range_or_nan_is_rejected() {
        assert_eq!(celsius_to_raw(150.5), None);
        assert_eq!(celsius_to_raw(-50.5), None);
        assert_eq!(celsius_to_raw(f32::NAN), None);
        assert_eq!(celsius_to_raw(f32::INFINITY), None);
    }

    #[test]
    fn modes_are_strict_and_case_insensitive() {
        assert_eq!(strict_extension_mode("pwm"), Some(ExtensionMode::Pwm));
        assert_eq!(strict_extension_mode("Off"), Some(ExtensionMode::Off));
        assert_eq!(strict_extension_mode("bogus"), None);
        assert_eq!(strict_extension_mode(""), None);
        assert_eq!(
            strict_relais_mode("hwrollershutter"),
            Some(RelaisMode::HwRollershutter)
        );
        // an extension mode that isn't a relay mode
        assert_eq!(strict_relais_mode("pwm"), None);
        assert_eq!(strict_relais_mode(&"x".repeat(40)), None);
    }

    #[test]
    fn hex_numbers_need_the_prefix() {
        assert_eq!(parse_hex_u64("0x10"), Some(16));
        assert_eq!(
            parse_hex_u64("0xbc95ee25bf580000"),
            Some(0xbc95ee25bf580000)
        );
        assert_eq!(parse_hex_u64("10"), None);
        assert_eq!(parse_hex_u64("0x"), None);
        assert_eq!(parse_hex_u64("0xzz"), None);
    }

    fn encode(bytes: &[u8]) -> std::string::String {
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = std::string::String::new();
        for chunk in bytes.chunks(3) {
            let b = [
                chunk[0],
                *chunk.get(1).unwrap_or(&0),
                *chunk.get(2).unwrap_or(&0),
            ];
            let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(TABLE[(n >> (18 - 6 * i) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }
}
