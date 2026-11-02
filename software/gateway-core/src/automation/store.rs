//! How the configuration is laid out on flash, and the index the gateway
//! keeps in RAM to find what a button does.
//!
//! Every item is one entry of a key/value map: the key is
//! `section << 8 | slot`, the value the item [`super::encode`]d. A slot is
//! the item's place in its section, 0 up to the section's capacity.
//! Outputs, motion lights and schedules are stored with their names
//! ([`Named`]), motion lights and schedules also with whether they are
//! enabled.

use super::switches::BUTTONS;
use super::{
    DeviceRef, MAX_MOTION_LIGHTS, MAX_OUTPUTS, MAX_SCHEDULES, MAX_SWITCH_CONFIGS, MAX_SWITCH_REFS,
};
use heapless::Vec;
use serde::{Deserialize, Serialize};

/// The sections of the store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    /// Settings of the automation as a whole (slot [`MASTER_SLOT`]).
    Meta = 0,
    Output = 1,
    /// The button bindings of the first version: no longer read.
    Retired = 2,
    Motion = 3,
    Schedule = 4,
    /// Switch configurations: which switches.
    Switch = 5,
    /// What the buttons of switch configurations 0–15 do ([`button_slot`]).
    ButtonsLow = 6,
    /// … and of 16–31.
    ButtonsHigh = 7,
    /// Notes on devices ([`super::devices`]).
    Devices = 8,
}

/// The slot of the master switch (a `bool`) in [`Section::Meta`].
pub const MASTER_SLOT: u8 = 0;

impl Section {
    pub const ALL: [Self; 9] = [
        Self::Meta,
        Self::Output,
        Self::Retired,
        Self::Motion,
        Self::Schedule,
        Self::Switch,
        Self::ButtonsLow,
        Self::ButtonsHigh,
        Self::Devices,
    ];

    /// How many slots the section has.
    pub const fn capacity(self) -> usize {
        match self {
            Self::Meta => 1,
            Self::Output => MAX_OUTPUTS,
            Self::Retired => 0,
            Self::Motion => MAX_MOTION_LIGHTS,
            Self::Schedule => MAX_SCHEDULES,
            Self::Switch => MAX_SWITCH_CONFIGS,
            Self::ButtonsLow | Self::ButtonsHigh => BUTTON_SECTION,
            Self::Devices => super::devices::MAX_DEVICES,
        }
    }

    /// The map key of `slot`.
    pub const fn key(self, slot: u8) -> u16 {
        (self as u16) << 8 | slot as u16
    }

    /// The section and slot of a map key; `None` for a key no section
    /// has (an entry of a newer firmware, say).
    pub fn split(key: u16) -> Option<(Self, u8)> {
        let section = *Self::ALL.get(usize::from(key >> 8))?;
        let slot = (key & 0xFF) as u8;
        (usize::from(slot) < section.capacity()).then_some((section, slot))
    }
}

/// An output, motion light or schedule as stored: with its name, and for
/// the last two whether they are enabled (for outputs, `enabled` is
/// unused).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Named<T> {
    pub name: super::Name,
    pub enabled: bool,
    pub item: T,
}

/// Which slots of a section are taken, one bit each.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Slots(u128);

impl Slots {
    pub const fn new() -> Self {
        Self(0)
    }

    pub fn take(&mut self, slot: u8) {
        if slot < 128 {
            self.0 |= 1 << slot;
        }
    }

    pub fn free(&mut self, slot: u8) {
        if slot < 128 {
            self.0 &= !(1 << slot);
        }
    }

    pub fn taken(self, slot: u8) -> bool {
        slot < 128 && self.0 & (1 << slot) != 0
    }

    /// The first slot that isn't taken, below `capacity`.
    pub fn first_free(self, capacity: usize) -> Option<u8> {
        (0..capacity.min(128) as u8).find(|&s| !self.taken(s))
    }

    pub fn count(self) -> u32 {
        self.0.count_ones()
    }

    pub fn iter(self) -> impl Iterator<Item = u8> {
        (0..128u8).filter(move |&s| self.taken(s))
    }
}

/// Which switch configurations a button event goes to, so that it finds
/// them without reading the flash: the switches of every configuration,
/// and which of its buttons do something.
#[derive(Debug)]
pub struct SwitchIndex {
    /// Device key and configuration slot.
    switches: Vec<(u32, u8), MAX_SWITCH_REFS, u8>,
    /// Per configuration slot, the buttons with actions, one bit each.
    buttons: [u8; MAX_SWITCH_CONFIGS],
}

impl Default for SwitchIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl SwitchIndex {
    pub const fn new() -> Self {
        Self {
            switches: Vec::new(),
            buttons: [0; MAX_SWITCH_CONFIGS],
        }
    }

    pub fn clear(&mut self) {
        self.switches.clear();
        self.buttons = [0; MAX_SWITCH_CONFIGS];
    }

    /// Notes the switches of configuration `slot` (replacing what it had).
    /// `false` if there are more switches in all than the index holds.
    pub fn set_switches(&mut self, slot: u8, switches: &[DeviceRef]) -> bool {
        self.switches.retain(|&(_, s)| s != slot);
        switches
            .iter()
            .all(|d| self.switches.push((d.key(), slot)).is_ok())
    }

    /// Notes whether `button` of configuration `slot` does something.
    pub fn set_button(&mut self, slot: u8, button: u8, used: bool) {
        if let Some(mask) = self.buttons.get_mut(usize::from(slot)) {
            if used {
                *mask |= 1 << (button & 7);
            } else {
                *mask &= !(1 << (button & 7));
            }
        }
    }

    /// Forgets configuration `slot`.
    pub fn remove(&mut self, slot: u8) {
        self.switches.retain(|&(_, s)| s != slot);
        self.set_buttons(slot, 0);
    }

    fn set_buttons(&mut self, slot: u8, mask: u8) {
        if let Some(m) = self.buttons.get_mut(usize::from(slot)) {
            *m = mask;
        }
    }

    /// The buttons of configuration `slot` that do something, one bit each.
    pub fn buttons(&self, slot: u8) -> u8 {
        self.buttons.get(usize::from(slot)).copied().unwrap_or(0)
    }

    /// The configurations whose `button` of switch `device` does
    /// something.
    pub fn find(&self, device: DeviceRef, button: u8) -> impl Iterator<Item = u8> + '_ {
        let key = device.key();
        self.switches
            .iter()
            .filter(move |&&(k, s)| {
                k == key && button < BUTTONS && self.buttons(s) & (1 << button) != 0
            })
            .map(|&(_, s)| s)
    }

    /// The configuration a switch belongs to, if any.
    pub fn config_of(&self, device: DeviceRef) -> Option<u8> {
        let key = device.key();
        self.switches
            .iter()
            .find(|&&(k, _)| k == key)
            .map(|&(_, s)| s)
    }
}

/// Where the actions of `button` of switch configuration `config` are
/// stored.
pub fn button_slot(config: u8, button: u8) -> (Section, u8) {
    let i = usize::from(config) * usize::from(BUTTONS) + usize::from(button % BUTTONS);
    if i < BUTTON_SECTION {
        (Section::ButtonsLow, i as u8)
    } else {
        (Section::ButtonsHigh, (i - BUTTON_SECTION) as u8)
    }
}

/// The switch configuration and button a slot of the button sections
/// stores; `None` for another section.
pub fn button_of(section: Section, slot: u8) -> Option<(u8, u8)> {
    let base = match section {
        Section::ButtonsLow => 0,
        Section::ButtonsHigh => BUTTON_SECTION,
        _ => return None,
    };
    let i = base + usize::from(slot);
    Some((
        (i / usize::from(BUTTONS)) as u8,
        (i % usize::from(BUTTONS)) as u8,
    ))
}

/// Slots of each of the two button sections.
const BUTTON_SECTION: usize = 128;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::output::{Output, Relay};
    use crate::automation::{decode, encode, Name};

    fn switch(id: u8) -> DeviceRef {
        DeviceRef { device_type: 4, id }
    }

    #[test]
    fn keys() {
        assert_eq!(Section::Output.key(3), 0x0103);
        assert_eq!(Section::split(0x0103), Some((Section::Output, 3)));
        assert_eq!(Section::split(0x0400 | 7), Some((Section::Schedule, 7)));
        // Past a section's capacity, or an unknown section.
        assert_eq!(Section::split(0x0400 | 8), None);
        assert_eq!(Section::split(0x0A00), None);
        // The bindings of the first version are left alone.
        assert_eq!(Section::split(0x0200), None);
        assert_eq!(
            Section::split(Section::ButtonsHigh.key(127)),
            Some((Section::ButtonsHigh, 127))
        );
    }

    #[test]
    fn button_slots() {
        assert_eq!(button_slot(0, 0), (Section::ButtonsLow, 0));
        assert_eq!(button_slot(15, 7), (Section::ButtonsLow, 127));
        assert_eq!(button_slot(16, 0), (Section::ButtonsHigh, 0));
        assert_eq!(button_slot(31, 7), (Section::ButtonsHigh, 127));
        for config in 0..MAX_SWITCH_CONFIGS as u8 {
            for button in 0..BUTTONS {
                let (section, slot) = button_slot(config, button);
                assert_eq!(button_of(section, slot), Some((config, button)));
            }
        }
        assert_eq!(button_of(Section::Output, 0), None);
    }

    #[test]
    fn slots() {
        let mut s = Slots::default();
        assert_eq!(s.first_free(4), Some(0));
        s.take(0);
        s.take(1);
        s.take(127);
        assert_eq!(s.first_free(4), Some(2));
        assert_eq!(s.count(), 3);
        s.free(0);
        assert_eq!(s.first_free(4), Some(0));
        assert_eq!(s.iter().collect::<std::vec::Vec<_>>(), [1, 127]);
        s.take(0);
        s.take(2);
        s.take(3);
        assert_eq!(s.first_free(4), None);
    }

    #[test]
    fn switch_index() {
        let mut index = SwitchIndex::new();
        assert!(index.set_switches(3, &[switch(12), switch(13)]));
        assert!(index.set_switches(5, &[switch(13)]));
        index.set_button(3, 1, true);
        index.set_button(5, 1, true);
        index.set_button(5, 2, true);
        let found = |d, b| index.find(switch(d), b).collect::<std::vec::Vec<_>>();
        assert_eq!(found(12, 1), [3]);
        assert_eq!(found(13, 1), [3, 5]);
        assert_eq!(found(12, 2), []);
        assert_eq!(found(14, 1), []);
        assert_eq!(index.config_of(switch(13)), Some(3));
        index.set_button(3, 1, false);
        index.remove(5);
        let found = |d, b| index.find(switch(d), b).collect::<std::vec::Vec<_>>();
        assert_eq!(found(13, 1), []);
        assert_eq!(index.buttons(5), 0);
        assert_eq!(index.config_of(switch(13)), Some(3));
    }

    #[test]
    fn stored_items_round_trip() {
        let output = Named {
            name: Name::try_from("stairs").unwrap(),
            enabled: false,
            item: Output::Relay(Relay {
                device: DeviceRef {
                    device_type: 5,
                    id: 0x80,
                },
                channel: 9,
                bank: 0,
            }),
        };
        let mut buffer = [0u8; 64];
        let bytes = encode(&output, &mut buffer).unwrap();
        assert!(bytes.len() < 20, "{}", bytes.len());
        assert_eq!(decode::<Named<Output>>(bytes), Some(output));
    }
}
