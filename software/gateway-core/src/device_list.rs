//! A passive, best-effort registry of every CAN node observed on the bus,
//! built purely by snooping traffic (`DeviceList` in the original C++).
//! Provides name<->id resolution for the MQTT topic translation in
//! [`crate::topics`] and a dump of everything seen for the web UI's
//! `/state.json`.

use crate::can_id::CanId;
use crate::can_message_type::CanMessageType;
use heapless::{String, Vec};

/// One tracked CAN node, keyed by [`CanId::device_key`]. Field sizes match
/// `DeviceList::DeviceListEntry` exactly (`custom_string[10]`,
/// `version[12]`, `uid0`/`uid1[8]`).
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceListEntry {
    /// `is_ng|group|device_type|device_id`, i.e. [`CanId::device_key`].
    pub id: u32,
    pub custom_string: String<10>,
    pub version: String<12>,
    /// Uptime-clock minutes at last observation (`esp_timer_get_time()`-derived
    /// in the original; the caller supplies "now" so this stays deterministic).
    pub last_seen_minutes: u32,
    pub uptime: u32,
    pub baudrate: u8,
    pub hwrev: u8,
    pub extension_mode: u8,
    pub uid0: [u8; 8],
    pub uid1: [u8; 8],
    pub relais_mode: u8,
    pub state: u8,
    pub error: u8,
}

impl DeviceListEntry {
    fn new(id: u32) -> Self {
        Self {
            id,
            custom_string: String::new(),
            version: String::new(),
            last_seen_minutes: 0,
            uptime: 0,
            baudrate: 0,
            hwrev: 0,
            extension_mode: 0,
            uid0: [0; 8],
            uid1: [0; 8],
            relais_mode: 0,
            state: 0,
            error: 0,
        }
    }

    /// Applies one observed CAN frame's effect on this entry
    /// (`update_device` in the original). `msg_type`s the entry doesn't
    /// track are ignored, matching the original `switch`'s `default: break;`.
    fn update(&mut self, msg_type: CanMessageType, data: &[u8], now_minutes: u32) {
        self.last_seen_minutes = now_minutes;
        match msg_type {
            CanMessageType::Available => {
                if let Some(&b) = data.first() {
                    self.state = b;
                }
            }
            CanMessageType::DeviceError => {
                if let Some(&b) = data.first() {
                    self.error = b;
                }
            }
            CanMessageType::Baudrate => {
                if let Some(&b) = data.first() {
                    self.baudrate = b;
                }
            }
            CanMessageType::DeviceUid0 => copy_clamped(&mut self.uid0, data),
            CanMessageType::DeviceUid1 => copy_clamped(&mut self.uid1, data),
            CanMessageType::ApplicationVersion => {
                if data.len() == 4 {
                    let major = u16::from_le_bytes([data[0], data[1]]);
                    let minor = u16::from_le_bytes([data[2], data[3]]);
                    self.version.clear();
                    let _ = write_version(&mut self.version, major, minor);
                }
            }
            CanMessageType::ApplicationVersionString => {
                if let Ok(s) = core::str::from_utf8(data) {
                    self.version.clear();
                    let _ = self.version.push_str(truncate(s, self.version.capacity()));
                }
            }
            CanMessageType::Uptime => {
                if data.len() >= 4 {
                    self.uptime = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                }
            }
            CanMessageType::CustomString => {
                if let Ok(s) = core::str::from_utf8(data) {
                    self.custom_string.clear();
                    let _ = self
                        .custom_string
                        .push_str(truncate(s, self.custom_string.capacity()));
                }
            }
            CanMessageType::RelaisMode => {
                if let Some(&b) = data.first() {
                    self.relais_mode = b;
                }
            }
            CanMessageType::HwRev => {
                if let Some(&b) = data.first() {
                    self.hwrev = b;
                }
            }
            CanMessageType::ExtensionMode => {
                if let Some(&b) = data.first() {
                    self.extension_mode = b;
                }
            }
            _ => {}
        }
    }
}

fn copy_clamped(dest: &mut [u8; 8], data: &[u8]) {
    let n = data.len().min(8);
    dest[..n].copy_from_slice(&data[..n]);
}

fn truncate(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

fn write_version(out: &mut String<12>, major: u16, minor: u16) -> core::fmt::Result {
    use core::fmt::Write;
    write!(out, "{major}.{minor}")
}

/// What the caller should do after [`DeviceTable::observe`] sees a frame
/// from a not-yet-known device: send a zero-length `RequestParameter`
/// remote-frame request to it, matching the original's
/// `m_can.send((identifier & 0xFFFFFF00) + 12, ..., request=true)` (`12` is
/// `RequestParameter`'s discriminant).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestParameters {
    pub device_key: u32,
}

/// Registry of every CAN node seen so far, capacity `N` (the original uses
/// a fixed `DEVICE_LIST_SIZE = 100`).
pub struct DeviceTable<const N: usize> {
    entries: Vec<DeviceListEntry, N>,
}

impl<const N: usize> Default for DeviceTable<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> DeviceTable<N> {
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Records one observed CAN frame (`DeviceList::dispatch`). Frames from
    /// `device_id == 0` (broadcast source, never a real device) are
    /// ignored. Returns [`RequestParameters`] exactly when this is the
    /// first time this device's key has been seen and there's still room
    /// in the table — the caller should send that request.
    pub fn observe(
        &mut self,
        id: CanId,
        data: &[u8],
        now_minutes: u32,
    ) -> Option<RequestParameters> {
        if id.device_id == 0 {
            return None;
        }
        let key = id.device_key();

        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == key) {
            entry.update(id.msg_type, data, now_minutes);
            return None;
        }

        let mut entry = DeviceListEntry::new(key);
        entry.update(id.msg_type, data, now_minutes);
        if self.entries.push(entry).is_ok() {
            Some(RequestParameters { device_key: key })
        } else {
            None
        }
    }

    /// The custom string for `key`, or `""` if unknown (`DeviceList::entry`).
    pub fn entry(&self, key: u32) -> &str {
        self.entries
            .iter()
            .find(|e| e.id == key)
            .map(|e| e.custom_string.as_str())
            .unwrap_or("")
    }

    /// Resolves an MQTT topic segment or `/control.json` `commandId` into a
    /// device key (`DeviceList::resolve`). A literal, **lowercase**
    /// `"0x"`-prefixed string parses as hex directly (matching the
    /// original's exact-case prefix check, distinct from
    /// [`crate::helper::hex_to_int`]'s case-insensitive one); anything else
    /// is looked up by custom string. Returns `None` if neither matches —
    /// the original returns the sentinel `0` here, which this crate
    /// represents as `None` instead of a magic value that also happens to
    /// look like a real (if degenerate) device key.
    pub fn resolve(&self, device_string: &str) -> Option<u32> {
        if let Some(hex) = device_string.strip_prefix("0x") {
            return u32::from_str_radix(hex, 16).ok();
        }
        self.entries
            .iter()
            .find(|e| e.custom_string.as_str() == device_string)
            .map(|e| e.id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &DeviceListEntry> {
        self.entries.iter()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(device_type: u8, device_id: u8, msg: CanMessageType) -> CanId {
        CanId::new(device_type, device_id, msg)
    }

    #[test]
    fn broadcast_source_is_never_observed() {
        let mut table: DeviceTable<8> = DeviceTable::new();
        let action = table.observe(id(5, 0, CanMessageType::Available), &[1], 0);
        assert_eq!(action, None);
        assert!(table.is_empty());
    }

    #[test]
    fn first_sighting_creates_entry_and_requests_parameters() {
        let mut table: DeviceTable<8> = DeviceTable::new();
        let can_id = id(5, 1, CanMessageType::Available);
        let action = table.observe(can_id, &[1], 42);
        assert_eq!(
            action,
            Some(RequestParameters {
                device_key: can_id.device_key()
            })
        );
        assert_eq!(table.len(), 1);
        let entry = table.iter().next().unwrap();
        assert_eq!(entry.state, 1);
        assert_eq!(entry.last_seen_minutes, 42);
    }

    #[test]
    fn second_sighting_updates_without_requesting_parameters_again() {
        let mut table: DeviceTable<8> = DeviceTable::new();
        let can_id = id(5, 1, CanMessageType::Available);
        table.observe(can_id, &[1], 0);
        let action = table.observe(id(5, 1, CanMessageType::HwRev), &[3], 10);
        assert_eq!(action, None);
        assert_eq!(table.len(), 1);
        let entry = table.iter().next().unwrap();
        assert_eq!(entry.hwrev, 3);
        assert_eq!(entry.last_seen_minutes, 10);
    }

    #[test]
    fn table_full_drops_new_devices_silently() {
        let mut table: DeviceTable<1> = DeviceTable::new();
        table.observe(id(5, 1, CanMessageType::Available), &[1], 0);
        let action = table.observe(id(5, 2, CanMessageType::Available), &[1], 0);
        assert_eq!(action, None);
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn uid_copy_clamps_to_8_bytes_even_for_oversized_input() {
        // The original's raw loop has no bounds check against the 8-byte
        // array; this deliberately clamps instead of allowing overflow.
        let mut table: DeviceTable<8> = DeviceTable::new();
        let oversized = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        table.observe(id(5, 1, CanMessageType::DeviceUid0), &oversized, 0);
        assert_eq!(table.iter().next().unwrap().uid0, [1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn application_version_formats_major_dot_minor() {
        let mut table: DeviceTable<8> = DeviceTable::new();
        let data = 2u16
            .to_le_bytes()
            .into_iter()
            .chain(7u16.to_le_bytes())
            .collect::<heapless::Vec<u8, 4>>();
        table.observe(id(5, 1, CanMessageType::ApplicationVersion), &data, 0);
        assert_eq!(table.iter().next().unwrap().version.as_str(), "2.7");
    }

    #[test]
    fn entry_returns_empty_string_for_unknown_key() {
        let table: DeviceTable<8> = DeviceTable::new();
        assert_eq!(table.entry(0x1234), "");
    }

    #[test]
    fn resolve_prefers_lowercase_0x_hex_over_custom_string_lookup() {
        let mut table: DeviceTable<8> = DeviceTable::new();
        let can_id = id(5, 1, CanMessageType::CustomString);
        table.observe(can_id, b"kitchen", 0);
        assert_eq!(table.resolve("0x10050100"), Some(0x1005_0100));
    }

    #[test]
    fn resolve_uppercase_0x_prefix_is_not_treated_as_hex() {
        // Regression test for the original's exact-case "0x" prefix check:
        // "0X..." falls through to the custom-string search instead.
        let table: DeviceTable<8> = DeviceTable::new();
        assert_eq!(table.resolve("0X10050100"), None);
    }

    #[test]
    fn resolve_falls_back_to_custom_string() {
        let mut table: DeviceTable<8> = DeviceTable::new();
        let can_id = id(5, 1, CanMessageType::CustomString);
        table.observe(can_id, b"kitchen", 0);
        assert_eq!(table.resolve("kitchen"), Some(can_id.device_key()));
        assert_eq!(table.resolve("unknown"), None);
    }
}
