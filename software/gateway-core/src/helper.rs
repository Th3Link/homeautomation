//! String/hex utilities ported from the original C++ `helper.cpp`.

use heapless::String;

/// Returns the `pos`-th `/`-delimited, non-empty segment of `s` (matching
/// `mqtt_split`'s `find_first_not_of('/', ...)` skip-empty-segments
/// behavior — consecutive or leading/trailing slashes never produce an
/// empty segment). Returns `""` if there is no such segment.
pub fn mqtt_split(s: &str, pos: usize) -> &str {
    s.split('/')
        .filter(|segment| !segment.is_empty())
        .nth(pos)
        .unwrap_or("")
}

/// Formats `n` as lowercase hex with no leading zero-padding and no `0x`
/// prefix (matches `toHexString`; callers that want the `0x` prefix used
/// throughout the MQTT topic scheme add it themselves, same as the
/// original call sites do).
pub fn to_hex_string(n: u32) -> String<8> {
    use core::fmt::Write;
    let mut s = String::new();
    let _ = write!(s, "{n:x}");
    s
}

/// Parses a hex string into a `u32`, tolerating an optional `0x`/`0X`
/// prefix (matches `hextoInt`, which uses `std::stoul(s, nullptr, 16)` —
/// base-16 `stoul` accepts and skips an optional `0x`/`0X` prefix).
pub fn hex_to_int(s: &str) -> Option<u32> {
    let digits = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    u32::from_str_radix(digits, 16).ok()
}

/// Splits an MQTT broker URI of the form `mqtt://host[:port]` into
/// `(host, port)`, defaulting the port to 1883. An IPv6 literal must be
/// bracketed, as in any URI: `mqtt://[fd00::1]:1883` yields `"fd00::1"`
/// (brackets stripped, ready for a resolver). Returns `None` for any other
/// scheme or an unparsable port.
pub fn parse_broker_uri(uri: &str) -> Option<(&str, u16)> {
    let rest = uri.strip_prefix("mqtt://")?;
    if let Some(bracketed) = rest.strip_prefix('[') {
        let (host, after) = bracketed.split_once(']')?;
        return match after.strip_prefix(':') {
            Some(port) => Some((host, port.parse().ok()?)),
            None if after.is_empty() => Some((host, 1883)),
            None => None,
        };
    }
    match rest.rsplit_once(':') {
        Some((host, port)) => Some((host, port.parse().ok()?)),
        None => Some((rest, 1883)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mqtt_split_returns_nth_segment() {
        assert_eq!(mqtt_split("canbus/relais_command/0x05", 0), "canbus");
        assert_eq!(
            mqtt_split("canbus/relais_command/0x05", 1),
            "relais_command"
        );
        assert_eq!(mqtt_split("canbus/relais_command/0x05", 2), "0x05");
    }

    #[test]
    fn mqtt_split_skips_empty_segments() {
        assert_eq!(mqtt_split("a//b", 1), "b");
        assert_eq!(mqtt_split("/a/b/", 0), "a");
    }

    #[test]
    fn mqtt_split_out_of_range_returns_empty() {
        assert_eq!(mqtt_split("a/b", 5), "");
        assert_eq!(mqtt_split("", 0), "");
    }

    #[test]
    fn to_hex_string_has_no_padding_or_prefix() {
        assert_eq!(to_hex_string(0).as_str(), "0");
        assert_eq!(to_hex_string(0x0A).as_str(), "a");
        assert_eq!(to_hex_string(0x1005_2A82).as_str(), "10052a82");
    }

    #[test]
    fn hex_to_int_accepts_optional_0x_prefix() {
        assert_eq!(hex_to_int("1a"), Some(0x1a));
        assert_eq!(hex_to_int("0x1a"), Some(0x1a));
        assert_eq!(hex_to_int("0X1A"), Some(0x1a));
    }

    #[test]
    fn hex_to_int_rejects_garbage() {
        assert_eq!(hex_to_int("not hex"), None);
        assert_eq!(hex_to_int(""), None);
    }

    #[test]
    fn broker_uri_defaults_port_and_parses_host() {
        assert_eq!(
            parse_broker_uri("mqtt://broker.lan"),
            Some(("broker.lan", 1883))
        );
        assert_eq!(
            parse_broker_uri("mqtt://10.0.0.5:1884"),
            Some(("10.0.0.5", 1884))
        );
    }

    #[test]
    fn broker_uri_accepts_bracketed_ipv6() {
        assert_eq!(
            parse_broker_uri("mqtt://[fd00::1]"),
            Some(("fd00::1", 1883))
        );
        assert_eq!(
            parse_broker_uri("mqtt://[fd00::1]:8883"),
            Some(("fd00::1", 8883))
        );
    }

    #[test]
    fn broker_uri_rejects_bad_input() {
        assert_eq!(parse_broker_uri("http://broker"), None);
        assert_eq!(parse_broker_uri("mqtt://host:notaport"), None);
        assert_eq!(parse_broker_uri("mqtt://[fd00::1"), None);
        assert_eq!(parse_broker_uri("mqtt://[fd00::1]junk"), None);
    }
}
