//! What the buttons of a wall switch do: one configuration per switch, or
//! for several switches that do the same (the two bedroom switches).
//!
//! A configuration is stored in parts: the [`SwitchConfig`] (which
//! switches) and one [`ButtonActions`] per button that does something
//! (what each kind of press does), so every part stays small enough for
//! one request and one flash item.

use super::binding::{Action, Event, Op};
use super::{invalid, valid_name, DeviceRef, Invalid};
use heapless::Vec;
use serde::{Deserialize, Serialize};

/// The most switches one configuration applies to.
pub const MAX_SWITCHES: usize = 8;
/// Buttons per switch: 0–3 on the switch, 4–7 on an extension board.
pub const BUTTONS: u8 = 8;
/// The most actions one press does.
pub const MAX_ACTIONS: usize = super::binding::MAX_ACTIONS;
/// Kinds of press: once, twice, three times, held, let go.
pub const MAX_EVENTS: usize = 5;

/// The switches a configuration applies to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwitchConfig {
    pub switches: Vec<DeviceRef, MAX_SWITCHES, u8>,
}

impl SwitchConfig {
    pub fn validate(&self) -> Result<(), Invalid> {
        if self.switches.is_empty() {
            return invalid("switches", "at least one");
        }
        if !self.switches.iter().all(|d| d.valid()) {
            return invalid("switches", "a device type below 64 and an id 1-255");
        }
        Ok(())
    }
}

/// What one kind of press does, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventActions {
    pub event: Event,
    pub actions: Vec<Action, MAX_ACTIONS, u8>,
}

/// What one button does, per kind of press.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ButtonActions {
    pub events: Vec<EventActions, MAX_EVENTS, u8>,
}

impl ButtonActions {
    pub fn validate(&self) -> Result<(), Invalid> {
        for (i, e) in self.events.iter().enumerate() {
            if self.events[..i].iter().any(|o| o.event == e.event) {
                return invalid("events.event", "each kind of press once");
            }
            if e.actions.is_empty() {
                return invalid("events.actions", "at least one");
            }
            for action in &e.actions {
                validate_action(action)?;
            }
        }
        Ok(())
    }

    /// The actions for `event`, if the button does something on it.
    pub fn actions(&self, event: Event) -> Option<&[Action]> {
        self.events
            .iter()
            .find(|e| e.event == event)
            .map(|e| e.actions.as_slice())
    }
}

pub(super) fn validate_action(action: &Action) -> Result<(), Invalid> {
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::Name;

    #[test]
    fn json_shape() {
        let json = r#"{"events":[
            {"event":"single","actions":[{"output":{"name":"wz","op":"toggle"}}]},
            {"event":"hold","actions":[{"output":{"name":"wz","op":"inc"}}]},
            {"event":"triple","actions":[{"automation":{"name":"flur","switch":"disable"}},
                                         {"output":{"name":"rollo","op":"down"}}]}]}"#;
        let (b, _): (ButtonActions, _) = serde_json_core::from_str(json).unwrap();
        assert_eq!(b.validate(), Ok(()));
        assert_eq!(
            b.actions(Event::Hold),
            Some(
                &[Action::Output {
                    name: Name::try_from("wz").unwrap(),
                    op: Op::Inc
                }][..]
            )
        );
        assert_eq!(b.actions(Event::Double), None);
        let (c, _): (SwitchConfig, _) =
            serde_json_core::from_str(r#"{"switches":[{"type":4,"id":12},{"type":4,"id":13}]}"#)
                .unwrap();
        assert_eq!(c.validate(), Ok(()));
    }

    #[test]
    fn an_event_only_once() {
        let json = r#"{"events":[
            {"event":"single","actions":[{"output":{"name":"wz","op":"on"}}]},
            {"event":"single","actions":[{"output":{"name":"wz","op":"off"}}]}]}"#;
        let (b, _): (ButtonActions, _) = serde_json_core::from_str(json).unwrap();
        assert_eq!(b.validate().unwrap_err().field, "events.event");
    }

    #[test]
    fn a_full_button_fits_a_flash_item() {
        let action = Action::Output {
            name: Name::try_from("abcdefghijklmnopqrst").unwrap(),
            op: Op::OnFor(0x00FF_FFFF),
        };
        let mut b = ButtonActions::default();
        for event in [
            Event::Single,
            Event::Double,
            Event::Triple,
            Event::Hold,
            Event::Released,
        ] {
            let mut actions = Vec::new();
            for _ in 0..MAX_ACTIONS {
                actions.push(action.clone()).unwrap();
            }
            b.events.push(EventActions { event, actions }).unwrap();
        }
        let mut buf = [0u8; 1024];
        let used = crate::automation::encode(&b, &mut buf).unwrap().len();
        assert!(used <= crate::automation::ITEM_MAX, "{used} bytes");
    }
}
