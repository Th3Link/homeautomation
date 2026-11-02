//! Button events and the actions they can do (see [`super::switches`]).

use super::{DeviceRef, Name};
use crate::button::ButtonEventKind;
use serde::{Deserialize, Serialize};

/// The most actions one button event has.
pub const MAX_ACTIONS: usize = 4;

/// The button events a binding reacts to (`Pressed` isn't one: every
/// press ends in one of these).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    Single,
    Double,
    Triple,
    /// Repeated every 500 ms while the button is held.
    Hold,
    /// The button let go after a hold.
    Released,
}

impl Event {
    pub fn from_kind(kind: ButtonEventKind) -> Option<Self> {
        match kind {
            ButtonEventKind::Single => Some(Self::Single),
            ButtonEventKind::Double => Some(Self::Double),
            ButtonEventKind::Tripple => Some(Self::Triple),
            ButtonEventKind::Hold => Some(Self::Hold),
            ButtonEventKind::Released => Some(Self::Released),
            ButtonEventKind::Pressed => None,
        }
    }

    fn index(self) -> u8 {
        match self {
            Self::Single => 0,
            Self::Double => 1,
            Self::Triple => 2,
            Self::Hold => 3,
            Self::Released => 4,
        }
    }
}

/// Which button event a binding reacts to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trigger {
    pub device: DeviceRef,
    /// 0–7: the switch's buttons (4–7 on an extension board).
    pub button: u8,
    pub event: Event,
}

impl Trigger {
    /// A compact key for the trigger (device key, button and event), for
    /// indexing the bindings.
    pub fn key(self) -> u32 {
        (self.device.key() & 0x1FFF_FF00)
            | (u32::from(self.button & 0x1F) << 3)
            | u32::from(self.event.index())
    }
}

/// What an action does to an output. Not every operation fits every
/// output; the ones that don't are ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Op {
    /// Lights and dimmers: the level they had last. Relays: on.
    On,
    Off,
    Toggle,
    /// Lights: that level; dimmers: that value.
    Set(u8),
    /// Lights: the top level; dimmers: 255.
    Max,
    /// Lights: one level brighter (from off: level 1).
    Inc,
    /// Lights: one level darker, down to off.
    Dec,
    /// Lights and dimmers: on at the last level, but at least this one
    /// (and brighter to it if on below it). Relays: on.
    AtLeast(u8),
    /// Relays: on for that many milliseconds; the node switches it off.
    OnFor(u32),
    /// Shutters: up, or stop if it is already moving up.
    Up,
    /// Shutters: down, or stop if it is already moving down.
    Down,
    /// Shutters: up, unless it is already moving up (while held).
    UpHold,
    /// Shutters: down, unless it is already moving down.
    DownHold,
    Stop,
}

/// Switching an automation (motion light or schedule) on or off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Switch {
    Enable,
    Disable,
    Toggle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Output {
        name: Name,
        op: Op,
    },
    Automation {
        name: Name,
        switch: Switch,
    },
    /// The night light LEDs of a light switch.
    Nightlight {
        device: DeviceRef,
        value: u8,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_shape() {
        let json = r#"[{"output":{"name":"wz","op":{"set":2}}},
                       {"output":{"name":"wz_sued1","op":"down"}},
                       {"output":{"name":"flur","op":{"at_least":4}}},
                       {"automation":{"name":"bad","switch":"disable"}}]"#;
        let (actions, _): (heapless::Vec<Action, 4>, _) = serde_json_core::from_str(json).unwrap();
        assert_eq!(
            actions[0],
            Action::Output {
                name: Name::try_from("wz").unwrap(),
                op: Op::Set(2)
            }
        );
        assert_eq!(
            actions[2],
            Action::Output {
                name: Name::try_from("flur").unwrap(),
                op: Op::AtLeast(4)
            }
        );
    }

    #[test]
    fn trigger_keys_differ() {
        let device = DeviceRef {
            device_type: 4,
            id: 12,
        };
        let t = |button, event| {
            Trigger {
                device,
                button,
                event,
            }
            .key()
        };
        let mut keys = std::vec::Vec::new();
        for button in 0..8 {
            for event in [
                Event::Single,
                Event::Double,
                Event::Triple,
                Event::Hold,
                Event::Released,
            ] {
                keys.push(t(button, event));
            }
        }
        keys.push(
            Trigger {
                device: DeviceRef {
                    device_type: 4,
                    id: 13,
                },
                button: 0,
                event: Event::Single,
            }
            .key(),
        );
        let n = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), n);
    }

    #[test]
    fn events() {
        assert_eq!(Event::from_kind(ButtonEventKind::Pressed), None);
        assert_eq!(
            Event::from_kind(ButtonEventKind::Tripple),
            Some(Event::Triple)
        );
    }
}
