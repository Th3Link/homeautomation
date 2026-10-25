//! Names for a node's temperature sensors, and which sensor has which role in
//! the heating controller. Pure bookkeeping — storing the result is the
//! hardware crate's job.
//!
//! The names are kept by the sensor's 48-bit serial, so they stay with the
//! sensor whatever slot (discovery order, see [`common_core::sensor_config`])
//! it has after a restart. The roles are kept the way the controller has
//! always had them, as the low 32 bits of the serial ([`RomCode::topic_id`]
//! — the id in the MQTT topics and on the console), so nothing stored before
//! changes meaning.
//!
//! [`RomCode::topic_id`]: crate::ds18b20::RomCode::topic_id

use crate::heating::TANK_SENSORS;
use crate::sensor_config::{SensorName, SensorRole, NAME_LEN};

/// Sensors that can have a name on one node.
pub const MAX_NAMED: usize = 16;
/// Bytes of one stored entry: the serial, then the name.
pub const ENTRY_LEN: usize = 6 + NAME_LEN;

/// Why a name could not be set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameError {
    /// Another sensor of the node has this name.
    Taken,
    /// [`MAX_NAMED`] sensors have names already.
    Full,
}

/// The names of up to [`MAX_NAMED`] sensors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SensorNames {
    entries: [Option<([u8; 6], SensorName)>; MAX_NAMED],
}

impl Default for SensorNames {
    fn default() -> Self {
        Self::new()
    }
}

impl SensorNames {
    pub const fn new() -> Self {
        Self {
            entries: [None; MAX_NAMED],
        }
    }

    /// The name of the sensor with this serial.
    pub fn get(&self, serial: &[u8; 6]) -> Option<SensorName> {
        self.entries
            .iter()
            .flatten()
            .find(|(known, _)| known == serial)
            .map(|(_, name)| *name)
    }

    /// Names a sensor (or renames it). Names are unique on a node.
    pub fn set(&mut self, serial: [u8; 6], name: SensorName) -> Result<usize, NameError> {
        if self
            .entries
            .iter()
            .flatten()
            .any(|(known, other)| *other == name && *known != serial)
        {
            return Err(NameError::Taken);
        }
        let index = self
            .index_of(&serial)
            .or_else(|| self.entries.iter().position(Option::is_none))
            .ok_or(NameError::Full)?;
        self.entries[index] = Some((serial, name));
        Ok(index)
    }

    /// Where the entry of `serial` is in the table (and so in flash).
    pub fn index_of(&self, serial: &[u8; 6]) -> Option<usize> {
        self.entries
            .iter()
            .position(|e| e.is_some_and(|(known, _)| known == *serial))
    }

    /// An entry as it is stored; `None` for an unused one.
    pub fn entry_bytes(&self, index: usize) -> Option<[u8; ENTRY_LEN]> {
        let (serial, name) = (*self.entries.get(index)?)?;
        let mut bytes = [0u8; ENTRY_LEN];
        bytes[..6].copy_from_slice(&serial);
        bytes[6..].copy_from_slice(&name.to_wire());
        Some(bytes)
    }

    /// Puts a stored entry back; one that doesn't make sense (a name that
    /// is not valid, a duplicate) is skipped.
    pub fn load_entry(&mut self, index: usize, bytes: &[u8]) {
        if index >= MAX_NAMED {
            return;
        }
        let Ok(bytes): Result<[u8; ENTRY_LEN], _> = bytes.try_into() else {
            return;
        };
        let serial: [u8; 6] = bytes[..6].try_into().expect("6 bytes");
        let wire: [u8; NAME_LEN] = bytes[6..].try_into().expect("7 bytes");
        let Some(name) = SensorName::from_wire(&wire) else {
            return;
        };
        let taken = self
            .entries
            .iter()
            .flatten()
            .any(|(known, other)| *other == name || *known == serial);
        if !taken {
            self.entries[index] = Some((serial, name));
        }
    }
}

/// The role of the sensor whose topic id is `id`, given who holds which role.
pub fn role_of(id: u32, tank: &[u32; TANK_SENSORS], flow: u32) -> SensorRole {
    if id == 0 {
        return SensorRole::None;
    }
    if flow == id {
        return SensorRole::Flow;
    }
    match tank.iter().position(|held| *held == id) {
        Some(position) => SensorRole::Tank(position as u8),
        None => SensorRole::None,
    }
}

/// Gives the sensor `id` the role `role`. It leaves the role it had, and
/// whoever had `role` before loses it. `false` for a tank position that
/// does not exist or an `id` of 0 (nothing to assign).
pub fn assign_role(
    id: u32,
    role: SensorRole,
    tank: &mut [u32; TANK_SENSORS],
    flow: &mut u32,
) -> bool {
    if id == 0 {
        return false;
    }
    if let SensorRole::Tank(position) = role {
        if usize::from(position) >= TANK_SENSORS {
            return false;
        }
    }
    for held in tank.iter_mut() {
        if *held == id {
            *held = 0;
        }
    }
    if *flow == id {
        *flow = 0;
    }
    match role {
        SensorRole::None => {}
        SensorRole::Tank(position) => tank[usize::from(position)] = id,
        SensorRole::Flow => *flow = id,
    }
    true
}

/// Whether `guard` (the first four serial bytes the sender expects) is the
/// beginning of `serial`.
pub fn guard_matches(serial: &[u8; 6], guard: &[u8; 4]) -> bool {
    serial[..4] == guard[..]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(text: &str) -> SensorName {
        SensorName::new(text).unwrap()
    }

    const A: [u8; 6] = [1, 2, 3, 4, 5, 6];
    const B: [u8; 6] = [9, 9, 9, 9, 9, 9];

    #[test]
    fn a_name_belongs_to_a_serial() {
        let mut names = SensorNames::new();
        assert_eq!(names.get(&A), None);
        names.set(A, name("TANK-O")).unwrap();
        names.set(B, name("VL")).unwrap();
        assert_eq!(names.get(&A), Some(name("TANK-O")));
        assert_eq!(names.get(&B), Some(name("VL")));
    }

    #[test]
    fn renaming_keeps_the_entry_and_names_are_unique() {
        let mut names = SensorNames::new();
        let first = names.set(A, name("ONE")).unwrap();
        names.set(B, name("TWO")).unwrap();
        assert_eq!(names.set(A, name("TWO")), Err(NameError::Taken));
        assert_eq!(names.set(A, name("UNO")), Ok(first));
        // Its own name again is not a clash.
        assert_eq!(names.set(A, name("UNO")), Ok(first));
        assert_eq!(names.get(&A), Some(name("UNO")));
    }

    #[test]
    fn the_table_is_limited() {
        let mut names = SensorNames::new();
        for i in 0..MAX_NAMED as u8 {
            let mut text = *b"S\0\0\0\0\0\0";
            text[1] = b'a' + i;
            names
                .set([i, 0, 0, 0, 0, 0], SensorName::from_wire(&text).unwrap())
                .unwrap();
        }
        assert_eq!(names.set(A, name("MORE")), Err(NameError::Full));
        // Renaming one that is there still works.
        assert!(names.set([0, 0, 0, 0, 0, 0], name("NEW")).is_ok());
    }

    #[test]
    fn entries_roundtrip_through_their_stored_form() {
        let mut names = SensorNames::new();
        names.set(A, name("TANK-O1")).unwrap();
        let index = names.index_of(&A).unwrap();
        let bytes = names.entry_bytes(index).unwrap();
        assert_eq!(&bytes[..6], &A);
        assert_eq!(&bytes[6..], b"TANK-O1");

        let mut loaded = SensorNames::new();
        loaded.load_entry(index, &bytes);
        assert_eq!(loaded.get(&A), Some(name("TANK-O1")));
        assert_eq!(names.entry_bytes(5), None);
    }

    #[test]
    fn broken_stored_entries_are_skipped() {
        let mut names = SensorNames::new();
        names.load_entry(0, &[1; 3]); // too short
        names.load_entry(0, &[1, 2, 3, 4, 5, 6, b'a', b' ', 0, 0, 0, 0, 0]); // bad name
        names.load_entry(99, &[0; ENTRY_LEN]); // no such entry
        assert_eq!(names, SensorNames::new());
        let mut good = [0u8; ENTRY_LEN];
        good[..6].copy_from_slice(&A);
        good[6..].copy_from_slice(b"X\0\0\0\0\0\0");
        names.load_entry(0, &good);
        // The same serial or name twice: the first stays.
        let mut dup = good;
        dup[..6].copy_from_slice(&B);
        names.load_entry(1, &dup);
        assert_eq!(names.get(&B), None);
    }

    #[test]
    fn the_role_of_a_sensor_follows_the_settings() {
        let tank = [0, 0x10, 0, 0, 0, 0, 0x20];
        assert_eq!(role_of(0x10, &tank, 0x30), SensorRole::Tank(1));
        assert_eq!(role_of(0x20, &tank, 0x30), SensorRole::Tank(6));
        assert_eq!(role_of(0x30, &tank, 0x30), SensorRole::Flow);
        assert_eq!(role_of(0x40, &tank, 0x30), SensorRole::None);
        assert_eq!(role_of(0, &tank, 0), SensorRole::None);
    }

    #[test]
    fn assigning_a_role_moves_the_sensor_and_displaces_the_old_holder() {
        let mut tank = [0; TANK_SENSORS];
        let mut flow = 0;
        assert!(assign_role(0x10, SensorRole::Tank(2), &mut tank, &mut flow));
        assert_eq!(tank[2], 0x10);
        // Another sensor takes the position: the first one has none now.
        assert!(assign_role(0x11, SensorRole::Tank(2), &mut tank, &mut flow));
        assert_eq!(tank[2], 0x11);
        assert_eq!(role_of(0x10, &tank, flow), SensorRole::None);
        // The same sensor moves to the flow role and leaves the tank.
        assert!(assign_role(0x11, SensorRole::Flow, &mut tank, &mut flow));
        assert_eq!((tank[2], flow), (0, 0x11));
        // None clears.
        assert!(assign_role(0x11, SensorRole::None, &mut tank, &mut flow));
        assert_eq!(flow, 0);
    }

    #[test]
    fn nothing_is_assigned_for_nothing() {
        let mut tank = [0; TANK_SENSORS];
        let mut flow = 0;
        assert!(!assign_role(0, SensorRole::Flow, &mut tank, &mut flow));
        assert!(!assign_role(5, SensorRole::Tank(7), &mut tank, &mut flow));
        assert_eq!((tank, flow), ([0; TANK_SENSORS], 0));
    }

    #[test]
    fn the_guard_is_the_start_of_the_serial() {
        assert!(guard_matches(&A, &[1, 2, 3, 4]));
        assert!(!guard_matches(&A, &[1, 2, 3, 5]));
        assert!(!guard_matches(&B, &[1, 2, 3, 4]));
    }
}
