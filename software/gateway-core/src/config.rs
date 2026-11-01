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
//! and hardcoded fallback values exactly (`hostname[40]`, `mqtt_uri[60]`,
//! `web_username[30]`, `custom_string[8]`, etc.). The original's WiFi
//! settings are gone along with WiFi itself — see ADR 0013.

use heapless::String;
use num_enum::{FromPrimitive, IntoPrimitive, TryFromPrimitive};

/// Keys for the values a storage backend persists. The discriminant is the
/// on-flash key, not just a Rust implementation detail — do not renumber
/// existing entries. 1-3 were the WiFi mode/SSID/password (ADR 0013) and
/// stay reserved rather than being reused.
#[derive(Debug, Copy, Clone, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
pub enum Key {
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
    /// The second MQTT broker (the first one's keys are `MqttEnabled`…
    /// `MqttPassword` above). See [`Key::broker`].
    Mqtt2Enabled = 17,
    Mqtt2Uri = 18,
    Mqtt2User = 19,
    Mqtt2Password = 20,
    /// Accept any server certificate on an `mqtts://` connection (encrypted,
    /// but not authenticated), per broker.
    MqttTlsInsecure = 21,
    Mqtt2TlsInsecure = 22,
    /// Publish Home Assistant discovery configurations
    /// ([`crate::ha_discovery`]) on the primary broker.
    HomeAssistant = 23,
}

/// How many MQTT brokers the gateway talks to at the same time.
pub const MQTT_BROKERS: usize = 2;

/// The storage keys of one MQTT broker slot.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct BrokerKeys {
    pub enabled: Key,
    pub uri: Key,
    pub user: Key,
    pub password: Key,
    pub tls_insecure: Key,
}

impl Key {
    /// The keys of MQTT broker slot `slot` (0 or 1; anything else is treated
    /// as the second slot).
    pub fn broker(slot: usize) -> BrokerKeys {
        if slot == 0 {
            BrokerKeys {
                enabled: Key::MqttEnabled,
                uri: Key::MqttUri,
                user: Key::MqttUser,
                password: Key::MqttPassword,
                tls_insecure: Key::MqttTlsInsecure,
            }
        } else {
            BrokerKeys {
                enabled: Key::Mqtt2Enabled,
                uri: Key::Mqtt2Uri,
                user: Key::Mqtt2User,
                password: Key::Mqtt2Password,
                tls_insecure: Key::Mqtt2TlsInsecure,
            }
        }
    }
}

/// One MQTT broker the gateway connects to.
#[derive(Debug, Clone, PartialEq)]
pub struct BrokerConfig {
    pub enabled: bool,
    /// `mqtt://host[:port]` or, with TLS, `mqtts://host[:port]`.
    pub uri: String<60>,
    pub user: String<60>,
    pub password: String<60>,
    /// For `mqtts://`: accept whatever certificate the broker presents
    /// instead of requiring the pinned one.
    pub tls_insecure: bool,
}

impl Default for BrokerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            uri: heapless_str("mqtt://IP:1883"),
            user: heapless_str(""),
            password: heapless_str(""),
            tls_insecure: false,
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
    pub hostname: String<40>,
    /// The MQTT brokers, all served at the same time: commands are accepted
    /// from each, and everything the gateway publishes goes to each.
    pub brokers: [BrokerConfig; MQTT_BROKERS],
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
            hostname: heapless_str("CAN2MQTTBridge"),
            brokers: [BrokerConfig::default(), BrokerConfig::default()],
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_original_hardcoded_values() {
        let cfg = GatewayConfig::default();
        assert_eq!(cfg.hostname.as_str(), "CAN2MQTTBridge");
        for broker in &cfg.brokers {
            assert!(!broker.enabled);
            assert_eq!(broker.uri.as_str(), "mqtt://IP:1883");
            assert!(!broker.tls_insecure);
        }
        assert_eq!(cfg.web_username.as_str(), "CAN2MQTTSETUP");
        assert_eq!(cfg.web_password.as_str(), "Can2MqttPass");
        assert_eq!(cfg.can_id, 0xFF);
        assert_eq!(cfg.can_type, 0xFF);
        assert_eq!(cfg.can_bitrate, Bitrate::B50);
        assert_eq!(cfg.update_delay_ms, 10);
    }

    #[test]
    fn bitrate_string_and_hz_parsing() {
        assert_eq!(Bitrate::from_name("b100"), Bitrate::B100);
        assert_eq!(Bitrate::from_name("nonsense"), Bitrate::B50);
        assert_eq!(Bitrate::from_hz(100_000), Bitrate::B100);
        assert_eq!(Bitrate::from_hz(1), Bitrate::B50);
    }

    #[test]
    fn key_discriminants_are_stable() {
        assert_eq!(u8::from(Key::Hostname), 4);
        assert_eq!(u8::from(Key::UpdateDelay), 16);
        assert_eq!(u8::from(Key::Mqtt2Enabled), 17);
        assert_eq!(u8::from(Key::Mqtt2Uri), 18);
        assert_eq!(u8::from(Key::Mqtt2User), 19);
        assert_eq!(u8::from(Key::Mqtt2Password), 20);
        assert_eq!(u8::from(Key::MqttTlsInsecure), 21);
        assert_eq!(u8::from(Key::Mqtt2TlsInsecure), 22);
    }

    #[test]
    fn broker_slots_use_their_own_keys() {
        let first = Key::broker(0);
        let second = Key::broker(1);
        assert_eq!(first.uri, Key::MqttUri);
        assert_eq!(second.uri, Key::Mqtt2Uri);
        // No key is shared between the slots.
        for (a, b) in [
            (first.enabled, second.enabled),
            (first.uri, second.uri),
            (first.user, second.user),
            (first.password, second.password),
            (first.tls_insecure, second.tls_insecure),
        ] {
            assert_ne!(u8::from(a), u8::from(b));
        }
    }
}
