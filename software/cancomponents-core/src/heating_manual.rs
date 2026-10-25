//! The manual (commissioning) mode of the heating controller: the relays
//! 6-10 (valves, mixer, pump) are taken out of the controller's hands and
//! switched one by one — to vent the circuit, say, which can take hours.
//!
//! The mode has no time limit: it stays on until it is turned off. It does
//! not survive a restart of the node (everything is off after one anyway).
//! The heating rod, relays 0-5, is never part of it.

use crate::heating::{LAST_CONTROLLED_RELAY, MIXER_COLD_RELAY, MIXER_HOT_RELAY};

/// First relay manual mode can switch; 0-5 are the heating rod.
pub const FIRST_MANUAL_RELAY: usize = 6;
/// Last relay manual mode can switch (relay 11 is not part of the
/// controller).
pub const LAST_MANUAL_RELAY: usize = LAST_CONTROLLED_RELAY;
/// How many relays manual mode has.
pub const MANUAL_RELAYS: usize = LAST_MANUAL_RELAY - FIRST_MANUAL_RELAY + 1;

/// Why a relay could not be switched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManualError {
    /// The controller has the relay; manual mode is off.
    NotManual,
    /// Only the relays 6-10 can be switched by hand.
    NotSwitchable,
    /// The other direction of the mixer is on.
    MixerInterlock,
}

impl ManualError {
    pub const fn message(self) -> &'static str {
        match self {
            Self::NotManual => "the controller drives this relay; turn manual mode on first",
            Self::NotSwitchable => "only relays 6-10 can be switched by hand",
            Self::MixerInterlock => "the other mixer direction is on; switch it off first",
        }
    }
}

/// Whether manual mode is on and which of its relays are on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ManualMode {
    enabled: bool,
    on: [bool; MANUAL_RELAYS],
}

impl ManualMode {
    pub const fn new() -> Self {
        Self {
            enabled: false,
            on: [false; MANUAL_RELAYS],
        }
    }

    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    /// Turns the mode on or off. Either way every relay of it is off
    /// afterwards: on a start they all go off, on an end the controller
    /// takes over from a known state. Turning it on when it is on already
    /// changes nothing, so a repeated request is harmless.
    pub fn set_enabled(&mut self, enabled: bool) {
        if enabled && self.enabled {
            return;
        }
        self.enabled = enabled;
        self.on = [false; MANUAL_RELAYS];
    }

    /// Switches one relay. Only in manual mode, only the relays 6-10, and
    /// never both mixer directions at once (switching one off is always
    /// allowed).
    pub fn switch(&mut self, relay: usize, on: bool) -> Result<(), ManualError> {
        if !(FIRST_MANUAL_RELAY..=LAST_MANUAL_RELAY).contains(&relay) {
            return Err(ManualError::NotSwitchable);
        }
        if !self.enabled {
            return Err(ManualError::NotManual);
        }
        if on {
            let opposite = match relay {
                MIXER_HOT_RELAY => Some(MIXER_COLD_RELAY),
                MIXER_COLD_RELAY => Some(MIXER_HOT_RELAY),
                _ => None,
            };
            if opposite.is_some_and(|other| self.is_on(other)) {
                return Err(ManualError::MixerInterlock);
            }
        }
        self.on[relay - FIRST_MANUAL_RELAY] = on;
        Ok(())
    }

    /// Whether `relay` (6-10) is on by hand.
    pub fn is_on(&self, relay: usize) -> bool {
        relay
            .checked_sub(FIRST_MANUAL_RELAY)
            .and_then(|index| self.on.get(index))
            .copied()
            .unwrap_or(false)
    }

    /// The relays that are on as bits, bit 0 = relay 6.
    pub fn relays_on_mask(&self) -> u8 {
        self.on
            .iter()
            .enumerate()
            .fold(0, |mask, (i, &on)| mask | (u8::from(on) << i))
    }
}

/// `true` for the relays manual mode owns (6-10).
pub const fn is_manual_relay(relay: usize) -> bool {
    relay >= FIRST_MANUAL_RELAY && relay <= LAST_MANUAL_RELAY
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on() -> ManualMode {
        let mut mode = ManualMode::new();
        mode.set_enabled(true);
        mode
    }

    #[test]
    fn nothing_can_be_switched_while_off() {
        let mut mode = ManualMode::new();
        assert_eq!(mode.switch(10, true), Err(ManualError::NotManual));
        assert_eq!(mode.relays_on_mask(), 0);
    }

    #[test]
    fn the_rod_and_relay_11_are_out_of_reach() {
        let mut mode = on();
        for relay in [0, 3, 5, 11, 12, 255] {
            assert_eq!(mode.switch(relay, true), Err(ManualError::NotSwitchable));
        }
        assert_eq!(mode.relays_on_mask(), 0);
    }

    #[test]
    fn relays_are_switched_one_by_one() {
        let mut mode = on();
        mode.switch(6, true).unwrap();
        mode.switch(10, true).unwrap();
        assert!(mode.is_on(6) && mode.is_on(10) && !mode.is_on(7));
        assert_eq!(mode.relays_on_mask(), 0b1_0001);
        mode.switch(6, false).unwrap();
        assert_eq!(mode.relays_on_mask(), 0b1_0000);
    }

    #[test]
    fn the_mixer_directions_exclude_each_other() {
        let mut mode = on();
        mode.switch(MIXER_HOT_RELAY, true).unwrap();
        assert_eq!(
            mode.switch(MIXER_COLD_RELAY, true),
            Err(ManualError::MixerInterlock)
        );
        // Off is always allowed, and then the other way works.
        mode.switch(MIXER_COLD_RELAY, false).unwrap();
        mode.switch(MIXER_HOT_RELAY, false).unwrap();
        mode.switch(MIXER_COLD_RELAY, true).unwrap();
        assert_eq!(
            mode.switch(MIXER_HOT_RELAY, true),
            Err(ManualError::MixerInterlock)
        );
    }

    #[test]
    fn starting_and_ending_switch_everything_off() {
        let mut mode = on();
        mode.switch(7, true).unwrap();
        mode.set_enabled(false);
        assert!(!mode.enabled());
        assert_eq!(mode.relays_on_mask(), 0);
        mode.set_enabled(true);
        assert_eq!(mode.relays_on_mask(), 0);
    }

    #[test]
    fn starting_again_keeps_what_is_on() {
        let mut mode = on();
        mode.switch(10, true).unwrap();
        mode.set_enabled(true);
        assert!(mode.is_on(10));
    }
}
