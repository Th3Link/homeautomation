//! Consolidated configuration schema.
//!
//! The original C++ gateway keeps ~17 independently-defaulted NVS scalar
//! and string entries scattered across `WiFi`, `MQTT`, `WebCredentials`,
//! `CAN`, `ConsoleCommandDevice`, and `CANUpdate` (each with its own inline
//! "read, or set-default-then-reread" boilerplate, and — per `WiFi`'s
//! setters not re-validating the same rule its own reader enforces — the
//! occasional inconsistency). This module is the single typed replacement:
//! one [`GatewayConfig`] struct with one set of defaults, and a [`Key`]
//! enum for whatever flash-backed storage `gateway-hardware` uses.
//!
//! String field capacities and defaults match the original's buffer sizes
//! and hardcoded fallback values exactly (`wifi_ssid[20]`, `hostname[40]`,
//! `mqtt_uri[60]`, `web_username[30]`, `custom_string[8]`, etc.) so
//! persisted values round-trip unchanged.

use heapless::String;
use num_enum::{FromPrimitive, IntoPrimitive, TryFromPrimitive};

/// Keys for the values a storage backend persists. The discriminant is the
/// on-flash key, not just a Rust implementation detail — do not renumber
/// existing entries.
#[derive(Debug, Copy, Clone, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
pub enum Key {
    WifiMode = 1,
    WifiSsid = 2,
    WifiPassword = 3,
    Hostname = 4,
    MqttEnabled = 5,
    MqttUri = 6,
    MqttUser = 7,
    MqttPassword = 8,
    WebUsername = 9,
    WebPassword = 10,
    CanId = 11,
    CanType = 12,
    CanBitrate = 13,
    HwRev = 14,
    CustomString = 15,
    UpdateDelay = 16,
}

/// `WiFi::Mode`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub enum WifiMode {
    Client,
    #[default]
    AccessPoint,
    Off,
}

impl WifiMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Client => "client",
            Self::AccessPoint => "ap",
            Self::Off => "off",
        }
    }

    /// Matches `WiFi::read_nvs`'s `wifi_mode` string interpretation:
    /// `"client"` -> [`WifiMode::Client`], `"ap"` -> [`WifiMode::AccessPoint`],
    /// anything else (including missing) -> [`WifiMode::Off`].
    pub fn from_name(s: &str) -> Self {
        match s {
            "client" => Self::Client,
            "ap" => Self::AccessPoint,
            _ => Self::Off,
        }
    }
}

/// `ICAN::BITRATE_t`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, IntoPrimitive, FromPrimitive)]
#[repr(u8)]
pub enum Bitrate {
    B22_222 = 1,
    B25 = 2,
    #[num_enum(default)]
    B50 = 3,
    B100 = 4,
}

impl Bitrate {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::B22_222 => "b22_222",
            Self::B25 => "b25",
            Self::B50 => "b50",
            Self::B100 => "b100",
        }
    }

    /// Matches `ICAN::bitrate(const char*)`: unrecognized names fall back
    /// to [`Bitrate::B50`].
    pub fn from_name(s: &str) -> Self {
        match s {
            "b22_222" => Self::B22_222,
            "b25" => Self::B25,
            "b100" => Self::B100,
            _ => Self::B50,
        }
    }

    /// Matches `ICAN::bitrate(unsigned int)`: unrecognized Hz values fall
    /// back to [`Bitrate::B50`].
    pub fn from_hz(hz: u32) -> Self {
        match hz {
            22_222 => Self::B22_222,
            25_000 => Self::B25,
            100_000 => Self::B100,
            _ => Self::B50,
        }
    }
}

/// The gateway's full persistent configuration, consolidating every NVS
/// key the original scatters across its modules. Defaults (via
/// [`Default`]) match the original's hardcoded fallback values exactly.
#[derive(Debug, Clone, PartialEq)]
pub struct GatewayConfig {
    pub wifi_mode: WifiMode,
    pub wifi_ssid: String<20>,
    pub wifi_password: String<20>,
    pub hostname: String<40>,
    pub mqtt_enabled: bool,
    pub mqtt_uri: String<60>,
    pub mqtt_user: String<60>,
    pub mqtt_password: String<60>,
    pub web_username: String<30>,
    pub web_password: String<30>,
    /// `0xFF` is the "not yet configured" sentinel, matching `CAN::CAN()`'s
    /// `m_id(0xFF)`.
    pub can_id: u8,
    /// `0xFF` is the "not yet configured" sentinel, matching `CAN::CAN()`'s
    /// `m_type(0xFF)`.
    pub can_type: u8,
    pub can_bitrate: Bitrate,
    pub hw_rev: u8,
    pub custom_string: String<8>,
    pub update_delay_ms: u32,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            wifi_mode: WifiMode::AccessPoint,
            wifi_ssid: heapless_str("CAN2MQTTSETUP"),
            wifi_password: heapless_str("Can2MqttPass"),
            hostname: heapless_str("CAN2MQTTBridge"),
            mqtt_enabled: false,
            mqtt_uri: heapless_str("mqtt://IP:1883"),
            mqtt_user: heapless_str(""),
            mqtt_password: heapless_str(""),
            web_username: heapless_str("CAN2MQTTSETUP"),
            web_password: heapless_str("Can2MqttPass"),
            can_id: 0xFF,
            can_type: 0xFF,
            can_bitrate: Bitrate::B50,
            hw_rev: 0,
            custom_string: heapless_str(""),
            update_delay_ms: crate::can_ota::DEFAULT_UPDATE_DELAY_MS,
        }
    }
}

fn heapless_str<const N: usize>(s: &str) -> String<N> {
    let mut out = String::new();
    let _ = out.push_str(s);
    out
}

/// Applies `WiFi::read_nvs`'s "password too short" rule: a WiFi password
/// under 8 characters can't actually be used (WPA2 requires >= 8), so the
/// original silently overrides both SSID and password back to the
/// hardcoded setup-AP fallback and forces access-point mode — *unless*
/// WiFi is already configured `Off`, which is left alone. Returns the
/// (possibly overridden) `(mode, ssid, password)` to actually bring up.
pub fn resolve_wifi_credentials(
    mode: WifiMode,
    ssid: &str,
    password: &str,
) -> (WifiMode, String<20>, String<20>) {
    if password.len() < 8 && mode != WifiMode::Off {
        (
            WifiMode::AccessPoint,
            heapless_str("CAN2MQTTSETUP"),
            heapless_str("Can2MqttPass"),
        )
    } else {
        (mode, heapless_str(ssid), heapless_str(password))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_original_hardcoded_values() {
        let cfg = GatewayConfig::default();
        assert_eq!(cfg.wifi_mode, WifiMode::AccessPoint);
        assert_eq!(cfg.wifi_ssid.as_str(), "CAN2MQTTSETUP");
        assert_eq!(cfg.wifi_password.as_str(), "Can2MqttPass");
        assert_eq!(cfg.hostname.as_str(), "CAN2MQTTBridge");
        assert!(!cfg.mqtt_enabled);
        assert_eq!(cfg.mqtt_uri.as_str(), "mqtt://IP:1883");
        assert_eq!(cfg.web_username.as_str(), "CAN2MQTTSETUP");
        assert_eq!(cfg.web_password.as_str(), "Can2MqttPass");
        assert_eq!(cfg.can_id, 0xFF);
        assert_eq!(cfg.can_type, 0xFF);
        assert_eq!(cfg.can_bitrate, Bitrate::B50);
        assert_eq!(cfg.update_delay_ms, 10);
    }

    #[test]
    fn wifi_mode_string_roundtrip() {
        assert_eq!(WifiMode::from_name("client"), WifiMode::Client);
        assert_eq!(WifiMode::from_name("ap"), WifiMode::AccessPoint);
        assert_eq!(WifiMode::from_name("garbage"), WifiMode::Off);
        assert_eq!(WifiMode::Client.as_str(), "client");
    }

    #[test]
    fn bitrate_string_and_hz_parsing() {
        assert_eq!(Bitrate::from_name("b100"), Bitrate::B100);
        assert_eq!(Bitrate::from_name("nonsense"), Bitrate::B50);
        assert_eq!(Bitrate::from_hz(100_000), Bitrate::B100);
        assert_eq!(Bitrate::from_hz(1), Bitrate::B50);
    }

    #[test]
    fn short_password_forces_ap_mode_and_fallback_credentials() {
        let (mode, ssid, pw) = resolve_wifi_credentials(WifiMode::Client, "home", "short");
        assert_eq!(mode, WifiMode::AccessPoint);
        assert_eq!(ssid.as_str(), "CAN2MQTTSETUP");
        assert_eq!(pw.as_str(), "Can2MqttPass");
    }

    #[test]
    fn long_enough_password_is_left_alone() {
        let (mode, ssid, pw) = resolve_wifi_credentials(WifiMode::Client, "home", "longenoughpw");
        assert_eq!(mode, WifiMode::Client);
        assert_eq!(ssid.as_str(), "home");
        assert_eq!(pw.as_str(), "longenoughpw");
    }

    #[test]
    fn off_mode_is_not_forced_into_ap_even_with_short_password() {
        let (mode, ..) = resolve_wifi_credentials(WifiMode::Off, "home", "short");
        assert_eq!(mode, WifiMode::Off);
    }

    #[test]
    fn key_discriminants_are_stable() {
        assert_eq!(u8::from(Key::WifiMode), 1);
        assert_eq!(u8::from(Key::UpdateDelay), 16);
    }
}
