//! The presence sensor (PIR) as the original C++ firmware reported it, in
//! the `PirSensor` message with the button layout
//! ([`crate::button_message`]), so the gateway's `canbus/presence/…` topic
//! keeps its meaning:
//!
//! - once a second with motion: `Pressed` with count 1 the first time, then
//!   `Hold` with count 2, 3, …;
//! - the first second without motion: `Released`, count 0; nothing after.

use crate::button_message::{ButtonMessage, ButtonState};

/// The number the presence sensor is reported as (the original's
/// `EXT_PIR`).
pub const PIR_NUM: u8 = 8;

pub struct PresenceFsm {
    state: ButtonState,
    count: u16,
}

impl Default for PresenceFsm {
    fn default() -> Self {
        Self::new()
    }
}

impl PresenceFsm {
    pub fn new() -> Self {
        Self {
            state: ButtonState::Released,
            count: 0,
        }
    }

    /// Once a second: whether there was motion during the second (the
    /// output went active at any time) or is now.
    pub fn tick(&mut self, motion: bool) -> Option<ButtonMessage> {
        if motion {
            self.state = if self.count == 0 {
                ButtonState::Pressed
            } else {
                ButtonState::Hold
            };
            self.count = self.count.saturating_add(1);
        } else {
            if self.state == ButtonState::Released {
                return None;
            }
            self.state = ButtonState::Released;
            self.count = 0;
        }
        Some(ButtonMessage::new(PIR_NUM, self.state, self.count))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motion_then_quiet() {
        let mut pir = PresenceFsm::new();
        assert_eq!(pir.tick(false), None);
        let m = pir.tick(true).unwrap();
        assert_eq!(
            (m.num, m.state, m.count),
            (PIR_NUM, ButtonState::Pressed, 1)
        );
        let m = pir.tick(true).unwrap();
        assert_eq!((m.state, m.count), (ButtonState::Hold, 2));
        let m = pir.tick(false).unwrap();
        assert_eq!((m.state, m.count), (ButtonState::Released, 0));
        assert_eq!(pir.tick(false), None);
        // A new motion starts over.
        assert_eq!(pir.tick(true).unwrap().state, ButtonState::Pressed);
    }
}
