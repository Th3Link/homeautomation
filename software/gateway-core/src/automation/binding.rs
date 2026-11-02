//! A button event and what it does: the gateway's version of a wall
//! switch's key table.

use super::{invalid, valid_name, DeviceRef, Invalid, Name};
use crate::button::ButtonEventKind;
use heapless::Vec;
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

/// A button event and its actions, done in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub trigger: Trigger,
    pub actions: Vec<Action, MAX_ACTIONS, u8>,
}

impl Binding {
    pub fn validate(&self) -> Result<(), Invalid> {
        if !self.trigger.device.valid() {
            return invalid("trigger.device", "a device type below 64 and an id 1-255");
        }
        if self.trigger.button > 31 {
            return invalid("trigger.button", "0-31");
        }
        if self.actions.is_empty() {
            return invalid("actions", "at least one");
        }
        for action in &self.actions {
            match action {
                Action::Output { name, op } => {
                    if !valid_name(name) {
                        return invalid("actions.name", "an output name");
                    }
                    if let Op::OnFor(ms) = op {
                        if *ms == 0 || *ms > 0x00FF_FFFF {
                            return invalid("actions.op.on_for", "1-16777215 ms");
                        }
                    }
                }
                Action::Automation { name, .. } if !valid_name(name) => {
                    return invalid("actions.name", "a motion light or schedule name");
                }
                Action::Nightlight { device, .. } if !device.valid() => {
                    return invalid("actions.device", "a device type below 64 and an id 1-255");
                }
                _ => {}
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_shape() {
        let json = r#"{"trigger":{"device":{"type":4,"id":12},"button":2,"event":"double"},
            "actions":[{"output":{"name":"wz","op":{"set":2}}},
                       {"output":{"name":"wz_sued1","op":"down"}},
                       {"automation":{"name":"bad","switch":"disable"}}]}"#;
        let (b, _): (Binding, _) = serde_json_core::from_str(json).unwrap();
        assert_eq!(b.trigger.event, Event::Double);
        assert_eq!(
            b.actions[0],
            Action::Output {
                name: Name::try_from("wz").unwrap(),
                op: Op::Set(2)
            }
        );
        assert_eq!(b.validate(), Ok(()));
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
    fn validation() {
        let b = Binding {
            trigger: Trigger {
                device: DeviceRef {
                    device_type: 4,
                    id: 12,
                },
                button: 0,
                event: Event::Single,
            },
            actions: Vec::new(),
        };
        assert_eq!(b.validate().unwrap_err().field, "actions");
        assert_eq!(Event::from_kind(ButtonEventKind::Pressed), None);
        assert_eq!(
            Event::from_kind(ButtonEventKind::Tripple),
            Some(Event::Triple)
        );
    }
}
