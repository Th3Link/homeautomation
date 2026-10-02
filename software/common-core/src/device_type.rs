use num_enum::{FromPrimitive, IntoPrimitive};

/// The kind of hardware a device is, reported in every
/// [`crate::can_id::CanId`]. Fixed wire-protocol enum (`ICAN::DEVICE_t` in
/// the original C++) — do not renumber existing variants. CAN node
/// firmware typically also persists its own value in its local config store
/// (e.g. `cancomponents-hardware`'s `config::Key::DeviceType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, IntoPrimitive, FromPrimitive)]
#[repr(u8)]
pub enum DeviceType {
    #[num_enum(default)]
    Unknown = 0,
    LegacyRelais = 2,
    LegacyLamps = 3,
    Button = 4,
    Relais = 5,
    Gateway = 6,
    Rollershutter = 7,
    SSR = 8,
}

impl DeviceType {
    /// Parses the human-readable names used by the `/control.json` RPC API
    /// and the console (`ICAN::device_type(std::string)`), e.g.
    /// `"Relais"`. Unrecognized names map to [`DeviceType::Unknown`],
    /// matching the C++ `if`/`else if` chain's fallthrough.
    pub fn from_name(s: &str) -> Self {
        match s {
            "LegacyRelais" => Self::LegacyRelais,
            "LegacyLamps" => Self::LegacyLamps,
            "Button" => Self::Button,
            "Relais" => Self::Relais,
            "Gateway" => Self::Gateway,
            "Rollershutter" => Self::Rollershutter,
            "SSR" => Self::SSR,
            _ => Self::Unknown,
        }
    }

    /// The inverse of [`DeviceType::from_name`] (`ICAN::device_string`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::LegacyRelais => "LegacyRelais",
            Self::LegacyLamps => "LegacyLamps",
            Self::Button => "Button",
            Self::Relais => "Relais",
            Self::Gateway => "Gateway",
            Self::Rollershutter => "Rollershutter",
            Self::SSR => "SSR",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_every_named_discriminant() {
        let known: &[(DeviceType, u8)] = &[
            (DeviceType::Unknown, 0),
            (DeviceType::LegacyRelais, 2),
            (DeviceType::LegacyLamps, 3),
            (DeviceType::Button, 4),
            (DeviceType::Relais, 5),
            (DeviceType::Gateway, 6),
            (DeviceType::Rollershutter, 7),
            (DeviceType::SSR, 8),
        ];
        for &(variant, byte) in known {
            assert_eq!(u8::from(variant), byte);
            assert_eq!(DeviceType::from(byte), variant);
        }
    }

    #[test]
    fn unknown_byte_falls_back_to_unknown() {
        assert_eq!(DeviceType::from(1), DeviceType::Unknown);
        assert_eq!(DeviceType::from(9), DeviceType::Unknown);
        assert_eq!(DeviceType::from(255), DeviceType::Unknown);
    }

    #[test]
    fn name_roundtrip() {
        for dt in [
            DeviceType::LegacyRelais,
            DeviceType::LegacyLamps,
            DeviceType::Button,
            DeviceType::Relais,
            DeviceType::Gateway,
            DeviceType::Rollershutter,
            DeviceType::SSR,
        ] {
            assert_eq!(DeviceType::from_name(dt.name()), dt);
        }
        assert_eq!(DeviceType::from_name("nonsense"), DeviceType::Unknown);
        assert_eq!(DeviceType::Unknown.name(), "Unknown");
    }
}
