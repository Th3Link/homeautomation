//! `canbus/debug/#`, `canbus/gateway/restart` — mirrors `BridgeDebug.cpp`.

use crate::helper::{hex_to_int, mqtt_split};
use crate::DecodeError;
use heapless::Vec;

pub const DEBUG_TOPIC: &str = "canbus/debug/#";

pub fn is_debug_topic(topic: &str) -> bool {
    topic.starts_with("canbus/debug/")
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Parses a `canbus/debug/#` message into `(target CAN id, raw payload
/// bytes)`: the target id is the topic's 3rd segment, hex
/// (`mqtt_split(topic, 2)`); the payload is everything in the body after
/// its first `/` (a leading count field, present but never actually used
/// beyond gating — see below), read as consecutive 2-hex-digit byte pairs,
/// capped at 8 bytes (the CAN frame limit).
///
/// The original reads the body from a **hardcoded character offset 2**
/// (`for (i = 2; i < data_len; i += 2)`), which only happens to land right
/// after the delimiter when the leading count field is exactly one
/// character — a multi-digit count would make it start mid-field and fail
/// to parse as hex. This instead finds the first `/` itself, so it
/// behaves the same for the common single-digit case and degrades
/// gracefully (a clean parse error, not misaligned byte reads) for a
/// multi-digit one. The leading count field itself is still required to
/// parse as an integer before anything else happens — mirroring how the
/// original's own (otherwise-unused) `std::stoi` call on it will throw
/// and abort the whole command if it doesn't, via the exception handler
/// wrapping the dispatcher fan-out.
pub fn parse_debug_command(topic: &str, data: &str) -> Result<(u32, Vec<u8, 8>), DecodeError> {
    let target_id = hex_to_int(mqtt_split(topic, 2)).ok_or(DecodeError)?;

    let (count_field, rest) = data.split_once('/').ok_or(DecodeError)?;
    let _count: i64 = count_field.parse().map_err(|_| DecodeError)?;

    let mut out = Vec::new();
    let bytes = rest.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() && out.len() < 8 {
        let hi = hex_nibble(bytes[i]).ok_or(DecodeError)?;
        let lo = hex_nibble(bytes[i + 1]).ok_or(DecodeError)?;
        out.push((hi << 4) | lo).ok();
        i += 2;
    }

    Ok((target_id, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_topic_prefix_matching() {
        assert!(is_debug_topic("canbus/debug/0x10052a80"));
    }

    #[test]
    fn parses_target_and_hex_payload() {
        let (id, bytes) = parse_debug_command("canbus/debug/1005", "3/AABBCC").unwrap();
        assert_eq!(id, 0x1005);
        assert_eq!(bytes.as_slice(), &[0xAA, 0xBB, 0xCC]);
    }

    #[test]
    fn caps_payload_at_8_bytes() {
        let (_, bytes) = parse_debug_command("canbus/debug/1", "9/0102030405060708090A").unwrap();
        assert_eq!(bytes.len(), 8);
    }

    #[test]
    fn rejects_missing_count_delimiter() {
        assert!(parse_debug_command("canbus/debug/1", "AABBCC").is_err());
    }

    #[test]
    fn rejects_non_integer_count_field() {
        assert!(parse_debug_command("canbus/debug/1", "x/AABBCC").is_err());
    }

    #[test]
    fn rejects_invalid_hex_payload() {
        assert!(parse_debug_command("canbus/debug/1", "3/ZZ").is_err());
    }

    #[test]
    fn rejects_unresolvable_target() {
        assert!(parse_debug_command("canbus/debug/not-hex", "3/AA").is_err());
    }
}
