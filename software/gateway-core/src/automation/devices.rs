//! What the gateway knows about a device beyond what it reports: which
//! sensors and extras it has on board (so the automation offers it only
//! where it fits), and the names of a lamp controller's lamps.

use super::{invalid, DeviceRef, Invalid};
use heapless::{String, Vec};
use serde::{Deserialize, Serialize};

/// Devices with notes.
pub const MAX_DEVICES: usize = 64;
/// Lamps of a lamp controller (bits of its mask).
pub const LAMPS: usize = 24;
/// The longest lamp name.
pub const LAMP_NAME_CAP: usize = 16;

pub type LampName = String<LAMP_NAME_CAP, u8>;

/// Something a device has on board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Feature {
    /// An extension board with buttons 4–7.
    Extension,
    /// A motion detector.
    Pir,
    /// Night light LEDs.
    Nightlight,
    /// Brightness sensor.
    Veml7700,
    Bme680,
    Bme280,
    Dht22,
    Scd30,
}

/// Up to one of each [`Feature`].
pub const MAX_FEATURES: usize = 8;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceNotes {
    #[serde(default)]
    pub features: Vec<Feature, MAX_FEATURES, u8>,
    /// Lamp controllers: the name of each lamp, by bit (empty: none).
    #[serde(default)]
    pub lamps: Vec<LampName, LAMPS, u8>,
}

impl DeviceNotes {
    pub fn validate(&self) -> Result<(), Invalid> {
        for (i, f) in self.features.iter().enumerate() {
            if self.features[..i].contains(f) {
                return invalid("features", "each once");
            }
        }
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.features.is_empty() && self.lamps.iter().all(|n| n.is_empty())
    }
}

/// As stored: with the device it is about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredNotes {
    pub device: DeviceRef,
    pub notes: DeviceNotes,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_shape_and_size() {
        let json = r#"{"features":["pir","veml7700","extension"],
            "lamps":["wz white 1","wz white 2","","wz red"]}"#;
        let (n, _): (DeviceNotes, _) = serde_json_core::from_str(json).unwrap();
        assert_eq!(n.validate(), Ok(()));
        assert_eq!(n.lamps[3], "wz red");
        let (twice, _): (DeviceNotes, _) =
            serde_json_core::from_str(r#"{"features":["pir","pir"]}"#).unwrap();
        assert!(twice.validate().is_err());

        let full = StoredNotes {
            device: DeviceRef {
                device_type: 3,
                id: 6,
            },
            notes: DeviceNotes {
                features: Vec::new(),
                lamps: (0..LAMPS)
                    .map(|_| LampName::try_from("abcdefghijklmnop").unwrap())
                    .collect(),
            },
        };
        let mut buf = [0u8; 1024];
        let used = crate::automation::encode(&full, &mut buf).unwrap().len();
        assert!(used <= crate::automation::ITEM_MAX, "{used}");
    }
}
