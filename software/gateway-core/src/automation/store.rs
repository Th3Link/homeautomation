//! How the configuration is laid out on flash, and the index the gateway
//! keeps in RAM to find a button's binding.
//!
//! Every item is one entry of a key/value map: the key is
//! `section << 8 | slot`, the value the item [`super::encode`]d. A slot is
//! the item's place in its section, 0 up to the section's capacity.
//! Outputs, motion lights and schedules are stored with their names
//! ([`Named`]), motion lights and schedules also with whether they are
//! enabled.

use super::binding::Trigger;
use super::{MAX_BINDINGS, MAX_MOTION_LIGHTS, MAX_OUTPUTS, MAX_SCHEDULES};
use heapless::Vec;
use serde::{Deserialize, Serialize};

/// The sections of the store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    /// Settings of the automation as a whole (slot [`MASTER_SLOT`]).
    Meta = 0,
    Output = 1,
    Binding = 2,
    Motion = 3,
    Schedule = 4,
}

/// The slot of the master switch (a `bool`) in [`Section::Meta`].
pub const MASTER_SLOT: u8 = 0;

impl Section {
    pub const ALL: [Self; 5] = [
        Self::Meta,
        Self::Output,
        Self::Binding,
        Self::Motion,
        Self::Schedule,
    ];

    /// How many slots the section has.
    pub const fn capacity(self) -> usize {
        match self {
            Self::Meta => 1,
            Self::Output => MAX_OUTPUTS,
            Self::Binding => MAX_BINDINGS,
            Self::Motion => MAX_MOTION_LIGHTS,
            Self::Schedule => MAX_SCHEDULES,
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

/// The triggers of the stored bindings, so that a button event finds its
/// bindings without reading the flash.
#[derive(Debug, Default)]
pub struct BindingIndex {
    entries: Vec<(u32, u8), MAX_BINDINGS, u8>,
}

impl BindingIndex {
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Notes that `slot` holds a binding for `trigger` (replacing what the
    /// slot held before).
    pub fn insert(&mut self, slot: u8, trigger: Trigger) {
        self.remove(slot);
        let _ = self.entries.push((trigger.key(), slot));
    }

    pub fn remove(&mut self, slot: u8) {
        self.entries.retain(|&(_, s)| s != slot);
    }

    /// The slots of the bindings for `trigger`, in slot order.
    pub fn find(&self, trigger: Trigger) -> Vec<u8, MAX_BINDINGS, u8> {
        let key = trigger.key();
        let mut slots: Vec<u8, MAX_BINDINGS, u8> = self
            .entries
            .iter()
            .filter(|&&(k, _)| k == key)
            .map(|&(_, s)| s)
            .collect();
        slots.sort_unstable();
        slots
    }

    /// The slot of a binding with exactly `trigger`, if there is one.
    pub fn slot_of(&self, trigger: Trigger) -> Option<u8> {
        self.find(trigger).first().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::binding::{Binding, Event};
    use crate::automation::output::{Output, Relay};
    use crate::automation::{decode, encode, DeviceRef, Name};

    fn trigger(button: u8, event: Event) -> Trigger {
        Trigger {
            device: DeviceRef {
                device_type: 4,
                id: 12,
            },
            button,
            event,
        }
    }

    #[test]
    fn keys() {
        assert_eq!(Section::Output.key(3), 0x0103);
        assert_eq!(Section::split(0x0103), Some((Section::Output, 3)));
        assert_eq!(Section::split(0x0400 | 7), Some((Section::Schedule, 7)));
        // Past a section's capacity, or an unknown section.
        assert_eq!(Section::split(0x0400 | 8), None);
        assert_eq!(Section::split(0x0900), None);
        assert_eq!(
            Section::split(Section::Binding.key(127)),
            Some((Section::Binding, 127))
        );
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
    fn binding_index() {
        let mut index = BindingIndex::new();
        index.insert(5, trigger(1, Event::Single));
        index.insert(2, trigger(1, Event::Single));
        index.insert(3, trigger(1, Event::Double));
        assert_eq!(index.find(trigger(1, Event::Single)).as_slice(), [2, 5]);
        assert_eq!(index.slot_of(trigger(1, Event::Double)), Some(3));
        assert!(index.find(trigger(2, Event::Single)).is_empty());
        // A slot gets a new trigger.
        index.insert(5, trigger(2, Event::Single));
        assert_eq!(index.find(trigger(1, Event::Single)).as_slice(), [2]);
        index.remove(2);
        assert!(index.find(trigger(1, Event::Single)).is_empty());
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

        let json = r#"{"trigger":{"device":{"type":4,"id":12},"button":1,"event":"single"},
            "actions":[{"output":{"name":"stairs","op":"toggle"}}]}"#;
        let (binding, _): (Binding, _) = serde_json_core::from_str(json).unwrap();
        let bytes = encode(&binding, &mut buffer).unwrap();
        assert_eq!(decode::<Binding>(bytes), Some(binding));
    }
}
