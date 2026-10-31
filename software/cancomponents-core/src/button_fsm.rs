//! Press/hold/click decisions for a physical button, made exactly the way
//! the original C++ firmware made them (esp-idf-lib's `button` driver,
//! polled every 10 ms, plus the firmware's `Button::dispatch`), so that old
//! and new nodes send the same `ButtonEvent`s and the gateway, MQTT topics
//! and the automations behind them don't notice which firmware runs:
//!
//! - **Press:** `Pressed`, count 0. For 50 ms after it, the level is
//!   ignored (bounce).
//! - **Clicks:** releasing starts a 250 ms window; `Single` (one release) or
//!   `Double` (two) is sent when it closes, `Tripple` right at the third
//!   release. Count 0. Further presses during a click sequence aren't sent.
//! - **Hold:** from 2 s on (1.5 s until autorepeat starts, then one
//!   interval), `Hold` every 500 ms with count 1, 2, …; letting go sends
//!   `Released` with the last count.
//!
//! Hardware-free: the caller polls the button's level every [`POLL_MS`] and
//! sends what [`ButtonFsm::poll`] returns.

use crate::button_message::{ButtonMessage, ButtonState};

/// How often the level is sampled.
pub const POLL_MS: u32 = 10;
/// After a press, the level is ignored this long.
const DEAD_TIME_MS: u32 = 50;
/// Held this long, the button starts repeating ...
const AUTOREPEAT_TIMEOUT_MS: u32 = 1500;
/// ... a `Hold` every this long.
const AUTOREPEAT_INTERVAL_MS: u32 = 500;
/// After a release, a further click is waited for this long.
const MULTI_CLICK_MS: u32 = 250;

/// The messages one poll produced (the click window closing and an edge can
/// fall into the same poll).
pub type Messages = heapless::Vec<ButtonMessage, 2>;

/// What the button driver reports (esp-idf-lib's `button_state_t`).
#[derive(Clone, Copy)]
enum DriverEvent {
    Pressed,
    Released,
    /// An autorepeat tick, and after every release.
    Clicked,
}

pub struct ButtonFsm {
    num: u8,
    // The driver's view.
    down: bool,
    pressed_ms: u32,
    repeating_ms: u32,
    // The firmware's view.
    state: ButtonState,
    count: u16,
    /// Time left in the click window.
    window_ms: Option<u32>,
}

impl ButtonFsm {
    /// A button reported as number `num` (0-3 on the board, 4-7 on the
    /// extension).
    pub fn new(num: u8) -> Self {
        Self {
            num,
            down: false,
            pressed_ms: 0,
            repeating_ms: 0,
            state: ButtonState::Released,
            count: 0,
            window_ms: None,
        }
    }

    /// One poll, [`POLL_MS`] after the previous one, with whether the
    /// button is pressed now.
    pub fn poll(&mut self, pressed: bool) -> Messages {
        let mut out = Messages::new();
        if let Some(left) = self.window_ms {
            let left = left.saturating_sub(POLL_MS);
            if left == 0 {
                self.window_ms = None;
                self.emit(&mut out);
                self.state = ButtonState::Released;
            } else {
                self.window_ms = Some(left);
            }
        }

        if self.down && self.pressed_ms < DEAD_TIME_MS {
            self.pressed_ms += POLL_MS;
            return out;
        }
        if pressed {
            if !self.down {
                self.down = true;
                self.pressed_ms = 0;
                self.repeating_ms = 0;
                self.dispatch(DriverEvent::Pressed, &mut out);
                return out;
            }
            self.pressed_ms += POLL_MS;
            if self.pressed_ms < AUTOREPEAT_TIMEOUT_MS {
                return out;
            }
            self.repeating_ms += POLL_MS;
            if self.repeating_ms >= AUTOREPEAT_INTERVAL_MS {
                self.repeating_ms = 0;
                self.dispatch(DriverEvent::Clicked, &mut out);
            }
        } else if self.down {
            self.down = false;
            self.dispatch(DriverEvent::Released, &mut out);
            self.dispatch(DriverEvent::Clicked, &mut out);
        }
        out
    }

    fn dispatch(&mut self, event: DriverEvent, out: &mut Messages) {
        match (event, self.state) {
            (DriverEvent::Pressed, ButtonState::Released) => {
                self.state = ButtonState::Pressed;
                self.count = 0;
                self.emit(out);
            }
            (DriverEvent::Released, ButtonState::Pressed) => {
                self.state = ButtonState::Single;
                self.window_ms = Some(MULTI_CLICK_MS);
            }
            (DriverEvent::Released, ButtonState::Single) => {
                self.state = ButtonState::Double;
                self.window_ms = Some(MULTI_CLICK_MS);
            }
            (DriverEvent::Released, ButtonState::Double) => {
                self.window_ms = None;
                self.state = ButtonState::Tripple;
                self.emit(out);
                self.state = ButtonState::Released;
            }
            (DriverEvent::Released, ButtonState::Hold) => {
                self.state = ButtonState::Released;
                self.emit(out);
            }
            (DriverEvent::Clicked, ButtonState::Pressed | ButtonState::Hold) => {
                self.state = ButtonState::Hold;
                self.count = self.count.wrapping_add(1);
                self.emit(out);
            }
            _ => {}
        }
    }

    fn emit(&self, out: &mut Messages) {
        // At most two per poll: the window closing, and one edge.
        let _ = out.push(ButtonMessage::new(self.num, self.state, self.count));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;
    use std::vec::Vec;

    /// Polls `fsm` with `pressed` for `ms` and collects (time, message).
    fn run(
        fsm: &mut ButtonFsm,
        at: &mut u32,
        pressed: bool,
        ms: u32,
        out: &mut Vec<(u32, ButtonMessage)>,
    ) {
        for _ in 0..ms / POLL_MS {
            *at += POLL_MS;
            for msg in fsm.poll(pressed) {
                out.push((*at, msg));
            }
        }
    }

    fn states(events: &[(u32, ButtonMessage)]) -> Vec<(ButtonState, u16)> {
        events.iter().map(|(_, m)| (m.state, m.count)).collect()
    }

    #[test]
    fn single_click() {
        let mut fsm = ButtonFsm::new(2);
        let (mut at, mut out) = (0, Vec::new());
        run(&mut fsm, &mut at, true, 100, &mut out);
        run(&mut fsm, &mut at, false, 400, &mut out);
        assert_eq!(
            states(&out),
            [(ButtonState::Pressed, 0), (ButtonState::Single, 0)]
        );
        assert!(out.iter().all(|(_, m)| m.num == 2));
        // Sent 250 ms after the release.
        assert_eq!(out[1].0, 110 + MULTI_CLICK_MS);
    }

    #[test]
    fn double_and_triple_click() {
        let mut fsm = ButtonFsm::new(0);
        let (mut at, mut out) = (0, Vec::new());
        for _ in 0..2 {
            run(&mut fsm, &mut at, true, 100, &mut out);
            run(&mut fsm, &mut at, false, 100, &mut out);
        }
        run(&mut fsm, &mut at, false, 300, &mut out);
        assert_eq!(
            states(&out),
            [(ButtonState::Pressed, 0), (ButtonState::Double, 0)]
        );

        out.clear();
        for _ in 0..3 {
            run(&mut fsm, &mut at, true, 100, &mut out);
            run(&mut fsm, &mut at, false, 100, &mut out);
        }
        // Tripple goes out right at the third release, nothing after it.
        let tripple_at = out[1].0;
        run(&mut fsm, &mut at, false, 300, &mut out);
        assert_eq!(
            states(&out),
            [(ButtonState::Pressed, 0), (ButtonState::Tripple, 0)]
        );
        assert_eq!(tripple_at, at - 300 - 100 + POLL_MS);
    }

    #[test]
    fn hold_repeats_and_released_carries_the_count() {
        let mut fsm = ButtonFsm::new(1);
        let (mut at, mut out) = (0, Vec::new());
        run(&mut fsm, &mut at, true, 2800, &mut out);
        run(&mut fsm, &mut at, false, 400, &mut out);
        assert_eq!(
            states(&out),
            [
                (ButtonState::Pressed, 0),
                (ButtonState::Hold, 1),
                (ButtonState::Hold, 2),
                (ButtonState::Released, 2),
            ]
        );
        // Pressed at 10 ms; the first hold 2 s later, then every 500 ms.
        assert_eq!(out[1].0, 2000);
        assert_eq!(out[2].0, 2500);
    }

    #[test]
    fn bounce_after_the_press_is_ignored() {
        let mut fsm = ButtonFsm::new(0);
        let (mut at, mut out) = (0, Vec::new());
        run(&mut fsm, &mut at, true, 10, &mut out);
        // Contact bounce within the dead time.
        run(&mut fsm, &mut at, false, 20, &mut out);
        run(&mut fsm, &mut at, true, 100, &mut out);
        run(&mut fsm, &mut at, false, 400, &mut out);
        assert_eq!(
            states(&out),
            [(ButtonState::Pressed, 0), (ButtonState::Single, 0)]
        );
    }

    #[test]
    fn click_then_hold_is_only_a_single() {
        let mut fsm = ButtonFsm::new(0);
        let (mut at, mut out) = (0, Vec::new());
        run(&mut fsm, &mut at, true, 100, &mut out);
        run(&mut fsm, &mut at, false, 100, &mut out);
        run(&mut fsm, &mut at, true, 3000, &mut out);
        run(&mut fsm, &mut at, false, 400, &mut out);
        assert_eq!(
            states(&out),
            [(ButtonState::Pressed, 0), (ButtonState::Single, 0)]
        );
    }
}
