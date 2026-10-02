//! Wire format for the `DeviceError` CAN message
//! (`ICAN::ERROR_t`/`DEVICE_ERROR` in the original C++).
//!
//! The only live producer in the original gateway is its CAN driver's own
//! bus-alert reporting (`CAN.cpp`'s receive-task loop): on any TWAI alert
//! (RX queue full, arbitration lost, bus error) it sends an 8-byte
//! `DeviceError` frame `[component, 0, 0, 0, alerts(LE 32-bit)]` with
//! `component = COMPONENT_CAN`. Everything else that observes
//! `DeviceError` (`DeviceList`) only ever reads the raw first byte for
//! display — it doesn't decode a structured report — so this module keeps
//! that byte-0-only path available too via [`component_byte`].

use num_enum::{FromPrimitive, IntoPrimitive};

/// `ICAN::ERROR_t` — identifies which subsystem raised a `DeviceError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, IntoPrimitive, FromPrimitive)]
#[repr(u8)]
pub enum ErrorComponent {
    #[num_enum(default)]
    Unknown = 0,
    FlashOverrun = 0x01,
    NoConfig = 0x02,
    DeviceIdTypeError = 0x03,
    FirmwareCorrupt = 0x04,
    Can = 0x05,
    Light = 0x06,
    Relais = 0x07,
    Main = 0x08,
    Update = 0x09,
    Nightlight = 0x0A,
    Ambient = 0x0B,
}

/// A `DeviceError` report as encoded by the CAN driver's bus-alert
/// reporting: `component` plus an opaque 32-bit detail value (TWAI alert
/// bitflags, for [`ErrorComponent::Can`]; unspecified for others).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceErrorReport {
    pub component: ErrorComponent,
    pub detail: u32,
}

impl DeviceErrorReport {
    /// Wire layout (8 bytes): `[component, reserved, reserved, reserved,
    /// detail(LE 32-bit)]`.
    pub fn to_bytes(self) -> [u8; 8] {
        let d = self.detail.to_le_bytes();
        [self.component.into(), 0, 0, 0, d[0], d[1], d[2], d[3]]
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, crate::DecodeError> {
        if data.len() != 8 {
            return Err(crate::DecodeError);
        }
        Ok(Self {
            component: ErrorComponent::from(data[0]),
            detail: u32::from_le_bytes([data[4], data[5], data[6], data[7]]),
        })
    }
}

/// Reads just the raw `component` byte out of a `DeviceError` payload,
/// matching how [`crate::device_list::DeviceTable`] tracks a node's last
/// reported error (`device.error = data[0]`, no structured decode).
/// Returns `0` for an empty payload rather than failing, matching the
/// original's unchecked `data[0]` read only ever being reached for
/// nonempty frames.
pub fn component_byte(data: &[u8]) -> u8 {
    data.first().copied().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_bytes_from_bytes_roundtrip() {
        let report = DeviceErrorReport {
            component: ErrorComponent::Can,
            detail: 0x0102_0304,
        };
        let bytes = report.to_bytes();
        assert_eq!(DeviceErrorReport::from_bytes(&bytes).unwrap(), report);
    }

    #[test]
    fn matches_can_driver_bus_alert_layout() {
        // Hand-derived from CAN.cpp's alert-reporting: component=COMPONENT_CAN(5),
        // reserved bytes 0, then `alerts` little-endian.
        let report = DeviceErrorReport {
            component: ErrorComponent::Can,
            detail: 0x0000_0003,
        };
        assert_eq!(report.to_bytes(), [5, 0, 0, 0, 3, 0, 0, 0]);
    }

    #[test]
    fn from_bytes_rejects_wrong_length() {
        assert!(DeviceErrorReport::from_bytes(&[0u8; 7]).is_err());
        assert!(DeviceErrorReport::from_bytes(&[0u8; 9]).is_err());
    }

    #[test]
    fn unknown_component_byte_falls_back() {
        assert_eq!(ErrorComponent::from(0xFF), ErrorComponent::Unknown);
    }

    #[test]
    fn component_byte_reads_first_byte_or_zero() {
        assert_eq!(component_byte(&[7, 1, 2]), 7);
        assert_eq!(component_byte(&[]), 0);
    }
}
