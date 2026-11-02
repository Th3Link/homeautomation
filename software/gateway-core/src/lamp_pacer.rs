//! Paces `LampGroup` commands for lamp controllers that can't take them
//! quickly.
//!
//! The legacy STM32 lamp controllers fade by themselves and handle one
//! change per second; commands sent faster confuse them. One change is a
//! short block of frames (openHAB sends up to four at once: the off masks
//! and the on masks of white and red), and that is fine. So the pacer
//! gathers what arrives for one controller, remembers the newest value
//! per lamp, and lets a block out at most once per interval.
//!
//! Merging per lamp keeps the result exact: whatever reaches the
//! controller in the end is the value each lamp was last given, and a
//! block is never larger than one frame per distinct value. Callers pass
//! in `now` (milliseconds); the gateway drives it from a task.

use crate::lamp::LampMsg;
use heapless::Vec;

/// The least time between two blocks to one controller.
pub const MIN_INTERVAL_MS: u64 = 1000;

/// How long frames are gathered into one block before it goes out, so
/// the frames of one change (sent back to back) stay together.
pub const GATHER_MS: u64 = 50;

const LAMPS: usize = 24;

/// Where a block goes: a controller and, for the 8-byte form, its bank.
/// `bank: None` is the 4-byte form without a bank.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub device_key: u32,
    pub bank: Option<u8>,
}

/// The frames to send to one controller now, at most one per distinct
/// value, off (`0`) first.
#[derive(Debug, PartialEq, Eq)]
pub struct Block {
    pub target: Target,
    pub frames: Vec<LampMsg, LAMPS>,
}

#[derive(Debug, Clone, Copy)]
struct Slot {
    target: Target,
    /// Lamps with a value still to send.
    pending: u32,
    values: [u8; LAMPS],
    due: Option<u64>,
    last_sent: Option<u64>,
}

/// Paces up to `N` controllers (or controller banks) at a time.
#[derive(Debug)]
pub struct LampPacer<const N: usize> {
    slots: [Option<Slot>; N],
    interval_ms: u64,
}

impl<const N: usize> LampPacer<N> {
    pub const fn new(interval_ms: u64) -> Self {
        Self {
            slots: [None; N],
            interval_ms,
        }
    }

    /// Takes `msg` in for `target`. Returns `false` if every slot is busy
    /// with another controller; the caller then sends it directly.
    pub fn push(&mut self, now: u64, target: Target, msg: LampMsg) -> bool {
        let interval = self.interval_ms;
        let index = self
            .slots
            .iter()
            .position(|s| s.is_some_and(|s| s.target == target))
            .or_else(|| self.slots.iter().position(Option::is_none))
            .or_else(|| {
                // A slot whose controller has nothing pending and whose
                // last block is long enough ago that it no longer matters.
                self.slots.iter().position(|s| {
                    s.is_some_and(|s| {
                        s.pending == 0 && s.last_sent.is_none_or(|t| now >= t + interval)
                    })
                })
            });
        let Some(index) = index else {
            return false;
        };
        let slot = self.slots[index].get_or_insert(Slot {
            target,
            pending: 0,
            values: [0; LAMPS],
            due: None,
            last_sent: None,
        });
        if slot.target != target {
            *slot = Slot {
                target,
                pending: 0,
                values: [0; LAMPS],
                due: None,
                last_sent: None,
            };
        }
        let mask = msg.bitmask & 0x00FF_FFFF;
        if mask == 0 {
            return true;
        }
        for (lamp, value) in slot.values.iter_mut().enumerate() {
            if mask & (1 << lamp) != 0 {
                *value = msg.value;
            }
        }
        slot.pending |= mask;
        if slot.due.is_none() {
            let gathered = now + GATHER_MS;
            slot.due = Some(match slot.last_sent {
                Some(t) => gathered.max(t + interval),
                None => gathered,
            });
        }
        true
    }

    /// When the next block is due, if any.
    pub fn next_due(&self) -> Option<u64> {
        self.slots.iter().flatten().filter_map(|s| s.due).min()
    }

    /// The earliest block due by `now`, if any. Call until `None`.
    pub fn take_due(&mut self, now: u64) -> Option<Block> {
        let slot = self
            .slots
            .iter_mut()
            .flatten()
            .filter(|s| s.due.is_some_and(|d| d <= now))
            .min_by_key(|s| s.due)?;
        let mut frames: Vec<LampMsg, LAMPS> = Vec::new();
        let mut left = slot.pending;
        while left != 0 {
            // Off first, then the values in ascending order.
            let value = (0..LAMPS)
                .filter(|&lamp| left & (1 << lamp) != 0)
                .map(|lamp| slot.values[lamp])
                .min()
                .unwrap_or(0);
            let mask = (0..LAMPS)
                .filter(|&lamp| left & (1 << lamp) != 0 && slot.values[lamp] == value)
                .fold(0u32, |mask, lamp| mask | (1 << lamp));
            left &= !mask;
            let _ = frames.push(LampMsg {
                value,
                bitmask: mask,
                bank: slot.target.bank.unwrap_or(0),
            });
        }
        slot.pending = 0;
        slot.due = None;
        slot.last_sent = Some(now);
        Some(Block {
            target: slot.target,
            frames,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WZ: Target = Target {
        device_key: 0x1003_0500,
        bank: None,
    };
    const BAD: Target = Target {
        device_key: 0x1003_0600,
        bank: None,
    };

    fn lamp(value: u8, bitmask: u32) -> LampMsg {
        LampMsg {
            value,
            bitmask,
            bank: 0,
        }
    }

    fn values(block: &Block) -> std::vec::Vec<(u8, u32)> {
        block.frames.iter().map(|f| (f.value, f.bitmask)).collect()
    }

    #[test]
    fn one_change_goes_out_as_one_block_after_gathering() {
        let mut p = LampPacer::<4>::new(MIN_INTERVAL_MS);
        // As openHAB sends a level: off white, off red, on white, on red.
        assert!(p.push(0, WZ, lamp(0, 0x41_A1A4 & !0x00_8100)));
        assert!(p.push(2, WZ, lamp(0, 0x82_5258 & !0x00_4200)));
        assert!(p.push(4, WZ, lamp(17, 0x00_8100)));
        assert!(p.push(6, WZ, lamp(34, 0x00_4200)));
        assert_eq!(p.next_due(), Some(GATHER_MS));
        assert_eq!(p.take_due(GATHER_MS - 1), None);
        let block = p.take_due(GATHER_MS).unwrap();
        assert_eq!(block.target, WZ);
        assert_eq!(
            values(&block),
            [
                (0, (0x41_A1A4 | 0x82_5258) & !0x00_C300),
                (17, 0x00_8100),
                (34, 0x00_4200)
            ]
        );
        assert_eq!(p.take_due(GATHER_MS), None);
        assert_eq!(p.next_due(), None);
    }

    #[test]
    fn changes_within_the_interval_merge_and_the_newest_wins() {
        let mut p = LampPacer::<4>::new(MIN_INTERVAL_MS);
        p.push(0, WZ, lamp(17, 0x0F));
        assert!(p.take_due(GATHER_MS).is_some());
        // Hold-dimming every 500 ms: 30, then 40, before the interval ends.
        p.push(500, WZ, lamp(30, 0x0F));
        p.push(1000, WZ, lamp(40, 0x0F));
        assert_eq!(p.next_due(), Some(GATHER_MS + MIN_INTERVAL_MS));
        assert_eq!(p.take_due(GATHER_MS + MIN_INTERVAL_MS - 1), None);
        let block = p.take_due(GATHER_MS + MIN_INTERVAL_MS).unwrap();
        assert_eq!(values(&block), [(40, 0x0F)]);
    }

    #[test]
    fn a_later_off_overrides_only_its_own_lamps() {
        let mut p = LampPacer::<4>::new(MIN_INTERVAL_MS);
        // Two rooms on the same controller (bath and WC share one).
        p.push(0, BAD, lamp(17, 0x11_0002));
        p.push(1, BAD, lamp(17, 0x40_0000));
        p.push(2, BAD, lamp(0, 0x40_0000));
        let block = p.take_due(GATHER_MS).unwrap();
        assert_eq!(values(&block), [(0, 0x40_0000), (17, 0x11_0002)]);
    }

    #[test]
    fn controllers_are_paced_separately() {
        let mut p = LampPacer::<4>::new(MIN_INTERVAL_MS);
        p.push(0, WZ, lamp(17, 1));
        p.push(10, BAD, lamp(17, 1));
        assert_eq!(p.take_due(GATHER_MS).unwrap().target, WZ);
        assert_eq!(p.take_due(GATHER_MS), None);
        assert_eq!(p.take_due(GATHER_MS + 10).unwrap().target, BAD);
        // Each one's interval runs from its own last block.
        p.push(100, WZ, lamp(0, 1));
        p.push(100, BAD, lamp(0, 1));
        assert_eq!(p.next_due(), Some(GATHER_MS + MIN_INTERVAL_MS));
        assert_eq!(p.take_due(GATHER_MS + MIN_INTERVAL_MS).unwrap().target, WZ);
        assert_eq!(p.next_due(), Some(GATHER_MS + 10 + MIN_INTERVAL_MS));
    }

    #[test]
    fn banks_are_separate_targets_and_keep_their_bank() {
        let mut p = LampPacer::<4>::new(MIN_INTERVAL_MS);
        let bank2 = Target {
            device_key: WZ.device_key,
            bank: Some(2),
        };
        p.push(0, WZ, lamp(10, 1));
        p.push(0, bank2, lamp(20, 1));
        let first = p.take_due(GATHER_MS).unwrap();
        let second = p.take_due(GATHER_MS).unwrap();
        assert_eq!((first.target, first.frames[0].value), (WZ, 10));
        assert_eq!((second.target, second.frames[0].bank), (bank2, 2));
    }

    #[test]
    fn full_slots_refuse_until_a_controller_is_idle() {
        let mut p = LampPacer::<1>::new(MIN_INTERVAL_MS);
        assert!(p.push(0, WZ, lamp(17, 1)));
        assert!(!p.push(0, BAD, lamp(17, 1)));
        p.take_due(GATHER_MS).unwrap();
        // Nothing pending, but WZ's interval is still running.
        assert!(!p.push(GATHER_MS, BAD, lamp(17, 1)));
        assert!(p.push(GATHER_MS + MIN_INTERVAL_MS, BAD, lamp(17, 1)));
        // BAD never sent before, so it only waits for the gathering.
        assert_eq!(p.next_due(), Some(2 * GATHER_MS + MIN_INTERVAL_MS));
    }

    #[test]
    fn an_empty_mask_sends_nothing() {
        let mut p = LampPacer::<2>::new(MIN_INTERVAL_MS);
        assert!(p.push(0, WZ, lamp(17, 0)));
        assert_eq!(p.next_due(), None);
    }
}
