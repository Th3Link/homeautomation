//! What the heating controllers say about their manual mode
//! ([`HeatingManualState`]), kept per device so that the web UI can show it
//! and the gateway can keep MQTT commands away from a heating that is being
//! worked on by hand.
//!
//! A controller in manual mode repeats its state every 5 s; one that stops
//! (a restart ends manual mode, and a lost connection says nothing) counts as
//! out of manual mode after [`STALE_AFTER_S`].

use common_core::heating_message::HeatingManualState;
use heapless::Vec;

/// A state older than this is not believed any more.
pub const STALE_AFTER_S: u32 = 15;

/// The manual mode of one device as far as it is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ManualStatus {
    pub enabled: bool,
    /// Bit 0 = relay 6 … bit 4 = relay 10.
    pub relays_on: u8,
}

/// The latest state of up to `N` heating controllers.
pub struct ManualTable<const N: usize> {
    entries: Vec<(u32, HeatingManualState, u32), N>,
}

impl<const N: usize> Default for ManualTable<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> ManualTable<N> {
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Records a state report from `device_key` at `now_s` (uptime seconds).
    pub fn observe(&mut self, device_key: u32, state: HeatingManualState, now_s: u32) {
        if let Some(known) = self.entries.iter_mut().find(|e| e.0 == device_key) {
            *known = (device_key, state, now_s);
        } else {
            // A full table (more controllers than expected) drops the report.
            let _ = self.entries.push((device_key, state, now_s));
        }
    }

    /// The manual mode of `device_key`: off if it never said or has not for
    /// [`STALE_AFTER_S`].
    pub fn status(&self, device_key: u32, now_s: u32) -> ManualStatus {
        self.entries
            .iter()
            .find(|e| e.0 == device_key)
            .filter(|e| now_s.saturating_sub(e.2) <= STALE_AFTER_S)
            .map(|e| ManualStatus {
                enabled: e.1.enabled,
                relays_on: e.1.relays_on,
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(enabled: bool, relays_on: u8) -> HeatingManualState {
        HeatingManualState { enabled, relays_on }
    }

    #[test]
    fn an_unknown_device_is_not_in_manual_mode() {
        let table: ManualTable<2> = ManualTable::new();
        assert_eq!(table.status(1, 100), ManualStatus::default());
    }

    #[test]
    fn the_latest_report_counts() {
        let mut table: ManualTable<2> = ManualTable::new();
        table.observe(1, state(true, 0b101), 10);
        assert_eq!(
            table.status(1, 12),
            ManualStatus {
                enabled: true,
                relays_on: 0b101
            }
        );
        table.observe(1, state(false, 0), 14);
        assert!(!table.status(1, 15).enabled);
    }

    #[test]
    fn a_silent_controller_is_out_of_manual_mode() {
        let mut table: ManualTable<2> = ManualTable::new();
        table.observe(1, state(true, 1), 100);
        assert!(table.status(1, 100 + STALE_AFTER_S).enabled);
        assert!(!table.status(1, 100 + STALE_AFTER_S + 1).enabled);
    }

    #[test]
    fn devices_are_kept_apart() {
        let mut table: ManualTable<2> = ManualTable::new();
        table.observe(1, state(true, 1), 0);
        table.observe(2, state(false, 0), 0);
        assert!(table.status(1, 1).enabled);
        assert!(!table.status(2, 1).enabled);
    }
}
