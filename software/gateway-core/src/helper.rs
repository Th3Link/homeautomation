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

/// Where an MQTT broker URI points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrokerAddress<'a> {
    /// `mqtts://` — the session is wrapped in TLS.
    pub tls: bool,
    /// Host name or IP literal (IPv6 without its brackets), ready for a
    /// resolver.
    pub host: &'a str,
    pub port: u16,
}

/// Splits an MQTT broker URI of the form `mqtt://host[:port]` (plain,
/// default port 1883) or `mqtts://host[:port]` (TLS, default port 8883).
/// An IPv6 literal must be bracketed, as in any URI: `mqtt://[fd00::1]:1883`
/// yields host `"fd00::1"`. Returns `None` for any other scheme or an
/// unparsable port.
pub fn parse_broker_uri(uri: &str) -> Option<BrokerAddress<'_>> {
    let (tls, rest, default_port) = if let Some(rest) = uri.strip_prefix("mqtt://") {
        (false, rest, 1883)
    } else {
        (true, uri.strip_prefix("mqtts://")?, 8883)
    };
    if let Some(bracketed) = rest.strip_prefix('[') {
        let (host, after) = bracketed.split_once(']')?;
        let port = match after.strip_prefix(':') {
            Some(port) => port.parse().ok()?,
            None if after.is_empty() => default_port,
            None => return None,
        };
        return Some(BrokerAddress { tls, host, port });
    }
    let (host, port) = match rest.rsplit_once(':') {
        Some((host, port)) => (host, port.parse().ok()?),
        None => (rest, default_port),
    };
    if host.is_empty() {
        return None;
    }
    Some(BrokerAddress { tls, host, port })
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

    fn address(tls: bool, host: &str, port: u16) -> Option<BrokerAddress<'_>> {
        Some(BrokerAddress { tls, host, port })
    }

    #[test]
    fn broker_uri_defaults_port_and_parses_host() {
        assert_eq!(
            parse_broker_uri("mqtt://broker.lan"),
            address(false, "broker.lan", 1883)
        );
        assert_eq!(
            parse_broker_uri("mqtt://10.0.0.5:1884"),
            address(false, "10.0.0.5", 1884)
        );
    }

    #[test]
    fn mqtts_scheme_selects_tls_and_its_default_port() {
        assert_eq!(
            parse_broker_uri("mqtts://broker.lan"),
            address(true, "broker.lan", 8883)
        );
        assert_eq!(
            parse_broker_uri("mqtts://10.0.0.5:8884"),
            address(true, "10.0.0.5", 8884)
        );
    }

    #[test]
    fn broker_uri_accepts_bracketed_ipv6() {
        assert_eq!(
            parse_broker_uri("mqtt://[fd00::1]"),
            address(false, "fd00::1", 1883)
        );
        assert_eq!(
            parse_broker_uri("mqtt://[fd00::1]:8884"),
            address(false, "fd00::1", 8884)
        );
        assert_eq!(
            parse_broker_uri("mqtts://[fd00::1]"),
            address(true, "fd00::1", 8883)
        );
    }

    #[test]
    fn broker_uri_rejects_bad_input() {
        assert_eq!(parse_broker_uri("http://broker"), None);
        assert_eq!(parse_broker_uri("mqtt://host:notaport"), None);
        assert_eq!(parse_broker_uri("mqtt://[fd00::1"), None);
        assert_eq!(parse_broker_uri("mqtt://[fd00::1]junk"), None);
        assert_eq!(parse_broker_uri("mqtt://"), None);
        assert_eq!(parse_broker_uri("mqtts://:8883"), None);
        assert_eq!(parse_broker_uri("ssl://broker"), None);
    }
}
