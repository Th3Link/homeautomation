//! What the nodes say about their temperature sensors
//! ([`SensorConfig`](crate::sensor_config::SensorConfig) info and name
//! frames): which sensor is in which slot, what it is called, which role it
//! has. The gateway keeps it to give sensors their names in MQTT topics, to
//! show them in the web UI, and to guard role assignments.
//!
//! Slots are temporary (see [`crate::sensor_config`]): an entry is replaced
//! when a node reports another sensor in the slot, and its name goes with
//! it. The list is asked for once per device (and again on request): the
//! table remembers when it did, so that a device is not asked over and over.

use crate::sensor_config::{SensorName, SensorRole};
use heapless::Vec;

/// What is known of one sensor of one device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SensorInfo {
    pub device_key: u32,
    pub slot: u8,
    pub serial: [u8; 6],
    pub name: Option<SensorName>,
    pub role: SensorRole,
    pub lost: bool,
    pub settling: bool,
}

impl SensorInfo {
    /// The serial as the 48-bit id the readings carry.
    pub fn sub_id(&self) -> u64 {
        let s = self.serial;
        u64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], 0, 0])
    }
}

/// How long to wait before asking the same device for its list again.
pub const ASK_AGAIN_AFTER_S: u32 = 60;

/// The sensors of up to `N` slots, and when each device was asked for them.
pub struct SensorInfoTable<const N: usize, const D: usize> {
    entries: Vec<SensorInfo, N>,
    asked: Vec<(u32, u32), D>,
}

impl<const N: usize, const D: usize> Default for SensorInfoTable<N, D> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize, const D: usize> SensorInfoTable<N, D> {
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
            asked: Vec::new(),
        }
    }

    /// Records an info frame. A different serial in the slot replaces the
    /// entry, name included; the same one keeps its name.
    pub fn observe_info(&mut self, info: SensorInfo) {
        match self
            .entries
            .iter_mut()
            .find(|e| e.device_key == info.device_key && e.slot == info.slot)
        {
            Some(known) => {
                let name = if known.serial == info.serial {
                    known.name
                } else {
                    None
                };
                *known = SensorInfo { name, ..info };
            }
            None => {
                // A full table (more sensors than expected) drops the report.
                let _ = self.entries.push(info);
            }
        }
    }

    /// Records a name frame for a slot whose info is known.
    pub fn observe_name(&mut self, device_key: u32, slot: u8, name: SensorName) {
        if let Some(known) = self
            .entries
            .iter_mut()
            .find(|e| e.device_key == device_key && e.slot == slot)
        {
            known.name = Some(name);
        }
    }

    pub fn get(&self, device_key: u32, slot: u8) -> Option<&SensorInfo> {
        self.entries
            .iter()
            .find(|e| e.device_key == device_key && e.slot == slot)
    }

    /// The sensor of `device_key` whose 48-bit id is `sub_id`.
    pub fn find(&self, device_key: u32, sub_id: u64) -> Option<&SensorInfo> {
        self.entries
            .iter()
            .find(|e| e.device_key == device_key && e.sub_id() == sub_id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &SensorInfo> {
        self.entries.iter()
    }

    /// Whether the list of `device_key` should be asked for now: it has not
    /// been, or not for [`ASK_AGAIN_AFTER_S`] and nothing is known of it.
    /// Says yes at most once per period; the caller then asks.
    pub fn should_ask(&mut self, device_key: u32, now_s: u32) -> bool {
        if self.entries.iter().any(|e| e.device_key == device_key) {
            return false;
        }
        match self.asked.iter_mut().find(|(key, _)| *key == device_key) {
            Some((_, at)) => {
                if now_s.saturating_sub(*at) >= ASK_AGAIN_AFTER_S {
                    *at = now_s;
                    true
                } else {
                    false
                }
            }
            None => {
                let _ = self.asked.push((device_key, now_s));
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: [u8; 6] = [1, 2, 3, 4, 5, 6];
    const B: [u8; 6] = [9, 9, 9, 9, 9, 9];

    fn info(device_key: u32, slot: u8, serial: [u8; 6]) -> SensorInfo {
        SensorInfo {
            device_key,
            slot,
            serial,
            name: None,
            role: SensorRole::None,
            lost: false,
            settling: false,
        }
    }

    fn name(text: &str) -> SensorName {
        SensorName::new(text).unwrap()
    }

    #[test]
    fn info_and_name_belong_together() {
        let mut table: SensorInfoTable<4, 2> = SensorInfoTable::new();
        table.observe_info(info(1, 0, A));
        table.observe_name(1, 0, name("VL"));
        assert_eq!(table.get(1, 0).unwrap().name, Some(name("VL")));
        // A name for an unknown slot is not kept.
        table.observe_name(1, 5, name("X"));
        assert!(table.get(1, 5).is_none());
    }

    #[test]
    fn a_refreshed_sensor_keeps_its_name_another_one_does_not() {
        let mut table: SensorInfoTable<4, 2> = SensorInfoTable::new();
        table.observe_info(info(1, 0, A));
        table.observe_name(1, 0, name("VL"));
        table.observe_info(SensorInfo {
            lost: true,
            ..info(1, 0, A)
        });
        let entry = table.get(1, 0).unwrap();
        assert!(entry.lost);
        assert_eq!(entry.name, Some(name("VL")));
        // The slot now has another sensor: the name was the old one's.
        table.observe_info(info(1, 0, B));
        assert_eq!(table.get(1, 0).unwrap().name, None);
    }

    #[test]
    fn sensors_are_found_by_their_id() {
        let mut table: SensorInfoTable<4, 2> = SensorInfoTable::new();
        table.observe_info(info(1, 3, A));
        table.observe_info(info(2, 3, B));
        let sub_id = u64::from_le_bytes([1, 2, 3, 4, 5, 6, 0, 0]);
        assert_eq!(table.find(1, sub_id).unwrap().slot, 3);
        assert!(table.find(2, sub_id).is_none());
        assert_eq!(table.get(2, 3).unwrap().serial, B);
    }

    #[test]
    fn a_device_is_asked_once_a_minute_until_it_answered() {
        let mut table: SensorInfoTable<4, 2> = SensorInfoTable::new();
        assert!(table.should_ask(1, 100));
        assert!(!table.should_ask(1, 101));
        assert!(!table.should_ask(1, 100 + ASK_AGAIN_AFTER_S - 1));
        assert!(table.should_ask(1, 100 + ASK_AGAIN_AFTER_S));
        // Another device has its own clock.
        assert!(table.should_ask(2, 101));
        // Once something is known, no more asking.
        table.observe_info(info(1, 0, A));
        assert!(!table.should_ask(1, 1000));
    }
}
