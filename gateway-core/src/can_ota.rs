//! Deterministic, pollable model of the CAN-bus OTA orchestration
//! (`CANUpdate` in the original C++): pushing firmware to a remote node
//! (or every node of a type) over the bus. Nothing here touches CAN or a
//! clock directly — callers get back a fixed sequence of "wait, then send
//! this frame" steps and drive them with real delays/sends in
//! `gateway-hardware`.
//!
//! The default per-write delay the original persists under
//! `config::Key::UpdateDelay` (`CANUpdate::UPDATE_DELAY_DEFAULT`).
pub const DEFAULT_UPDATE_DELAY_MS: u32 = 10;

use crate::can_message_type::CanMessageType;
use embassy_time::Duration;
use heapless::Vec;

/// One frame to send, with how long to wait beforehand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanOtaStep {
    /// How long to wait before sending this frame.
    pub wait: Duration,
    pub msg_type: CanMessageType,
    pub data: Vec<u8, 8>,
    /// Whether to send as a remote-frame request (RTR) rather than a data frame.
    pub request: bool,
}

fn step(wait_ms: u64, msg_type: CanMessageType, data: &[u8], request: bool) -> CanOtaStep {
    let mut v = Vec::new();
    v.extend_from_slice(data).ok();
    CanOtaStep {
        wait: Duration::from_millis(wait_ms),
        msg_type,
        data: v,
        request,
    }
}

/// The `0x10000000 + (device_type << 16)` broadcast-to-type address used by
/// `CANUpdate::by_type_start` — device_id left at `0` (broadcast within
/// that type).
pub fn base_key_for_type(device_type: u8) -> u32 {
    0x1000_0000 | ((device_type as u32) << 16)
}

/// The full CAN identifier (as `u32`) for sending `msg_type` to the device
/// (or type-broadcast) addressed by `base_key` — `base_key` must have its
/// `msg_type` bits zeroed already (e.g. from [`base_key_for_type`] or
/// [`crate::can_id::CanId::device_key`]/[`crate::device_list::DeviceTable::resolve`]).
pub fn target_id(base_key: u32, msg_type: CanMessageType) -> u32 {
    base_key + u8::from(msg_type) as u32
}

/// The fixed 5-step sequence `CANUpdate::start` sends: switch the target
/// into update mode, select/erase its flash, and announce the incoming
/// image's size/CRC — ending with a `FlashVerify` poll. Call
/// [`start_trailing_delay`] for the wait after the last step before
/// streaming data with a [`FlashWriter`].
pub fn start_steps(filesize: u32, crc: u32) -> [CanOtaStep; 5] {
    let fsz = filesize.to_be_bytes();
    let crc_be = crc.to_be_bytes();
    [
        step(0, CanMessageType::Restart, &[2 /* UPDATE_MODE */], false),
        step(
            2000,
            CanMessageType::FlashSelect,
            &[0, 0, 0, 0, fsz[0], fsz[1], fsz[2], fsz[3]],
            false,
        ),
        step(
            0,
            CanMessageType::FlashStart,
            &[
                crc_be[0], crc_be[1], crc_be[2], crc_be[3], fsz[0], fsz[1], fsz[2], fsz[3],
            ],
            false,
        ),
        step(500, CanMessageType::FlashErase, &[], false),
        step(5000, CanMessageType::FlashVerify, &[], true),
    ]
}

/// The wait after [`start_steps`]'s last step, before the caller should
/// start streaming firmware bytes with [`FlashWriter`].
pub fn start_trailing_delay() -> Duration {
    Duration::from_millis(500)
}

/// The fixed 3-step sequence `CANUpdate::complete` sends once every byte
/// has been written: mark the transfer complete, poll `FlashVerify`, then
/// (after a delay for the target to reboot into the new image) restart it
/// into application mode.
pub fn complete_steps() -> [CanOtaStep; 3] {
    [
        step(1000, CanMessageType::FlashComplete, &[], false),
        step(0, CanMessageType::FlashVerify, &[], true),
        step(2000, CanMessageType::Restart, &[1 /* APPLICATION */], false),
    ]
}

/// One `FlashWrite` chunk to send, with how long to wait beforehand and
/// (if this chunk crossed a 4096-byte boundary) an extra pause afterward
/// to let the target catch up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteChunk {
    pub wait: Duration,
    pub bytes: Vec<u8, 8>,
    pub extra_wait_after: Option<Duration>,
}

/// Streams firmware bytes as `FlashWrite` CAN frames (`CANUpdate::data`),
/// pacing them by `update_delay_ms` per frame plus an extra 1s pause every
/// 4096 bytes so slower targets can keep up. Stateful across calls to
/// [`FlashWriter::next_chunk`] — construct one per OTA session (`byte
/// count` and `remaining filesize` both reset at
/// [`CANUpdate::start`](start_steps)-time in the original).
pub struct FlashWriter {
    remaining_filesize: u32,
    byte_count: u32,
    update_delay_ms: u32,
}

impl FlashWriter {
    pub fn new(filesize: u32, update_delay_ms: u32) -> Self {
        Self {
            remaining_filesize: filesize,
            byte_count: 0,
            update_delay_ms,
        }
    }

    /// Takes up to 8 bytes off the front of `data` (advancing it) and
    /// returns the [`WriteChunk`] to send for them, or `None` once `data`
    /// is empty or the declared `filesize` has been fully consumed
    /// (matching `while (remaining > 0 && m_filesize > 0)`).
    pub fn next_chunk(&mut self, data: &mut &[u8]) -> Option<WriteChunk> {
        if data.is_empty() || self.remaining_filesize == 0 {
            return None;
        }
        let to_send = data.len().min(8).min(self.remaining_filesize as usize);
        let (chunk, rest) = data.split_at(to_send);
        *data = rest;

        let mut bytes = Vec::new();
        bytes.extend_from_slice(chunk).ok();

        self.remaining_filesize -= to_send as u32;
        self.byte_count += to_send as u32;
        let extra_wait_after = self
            .byte_count
            .is_multiple_of(4096)
            .then(|| Duration::from_millis(1000));

        Some(WriteChunk {
            wait: Duration::from_millis(self.update_delay_ms as u64),
            bytes,
            extra_wait_after,
        })
    }

    pub fn remaining_filesize(&self) -> u32 {
        self.remaining_filesize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_key_for_type_sets_is_ng_and_type_only() {
        assert_eq!(base_key_for_type(0x05), 0x1005_0000);
    }

    #[test]
    fn target_id_adds_msg_type_to_base_key() {
        assert_eq!(
            target_id(0x1005_0100, CanMessageType::FlashWrite),
            0x1005_0100 + 19
        );
    }

    #[test]
    fn start_steps_match_original_wire_bytes() {
        let steps = start_steps(0x0000_1234, 0xDEAD_BEEF);
        assert_eq!(steps[0].msg_type, CanMessageType::Restart);
        assert_eq!(steps[0].data.as_slice(), &[2]);
        assert_eq!(steps[0].wait, Duration::from_millis(0));

        assert_eq!(steps[1].msg_type, CanMessageType::FlashSelect);
        assert_eq!(steps[1].wait, Duration::from_millis(2000));
        assert_eq!(
            steps[1].data.as_slice(),
            &[0, 0, 0, 0, 0x00, 0x00, 0x12, 0x34]
        );

        assert_eq!(steps[2].msg_type, CanMessageType::FlashStart);
        assert_eq!(steps[2].wait, Duration::from_millis(0));
        assert_eq!(
            steps[2].data.as_slice(),
            &[0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x00, 0x12, 0x34]
        );

        assert_eq!(steps[3].msg_type, CanMessageType::FlashErase);
        assert_eq!(steps[3].wait, Duration::from_millis(500));
        assert!(steps[3].data.is_empty());
        assert!(!steps[3].request);

        assert_eq!(steps[4].msg_type, CanMessageType::FlashVerify);
        assert_eq!(steps[4].wait, Duration::from_millis(5000));
        assert!(steps[4].data.is_empty());
        assert!(steps[4].request);

        assert_eq!(start_trailing_delay(), Duration::from_millis(500));
    }

    #[test]
    fn complete_steps_match_original_sequence() {
        let steps = complete_steps();
        assert_eq!(steps[0].msg_type, CanMessageType::FlashComplete);
        assert_eq!(steps[0].wait, Duration::from_millis(1000));
        assert!(!steps[0].request);

        assert_eq!(steps[1].msg_type, CanMessageType::FlashVerify);
        assert_eq!(steps[1].wait, Duration::from_millis(0));
        assert!(steps[1].request);

        assert_eq!(steps[2].msg_type, CanMessageType::Restart);
        assert_eq!(steps[2].wait, Duration::from_millis(2000));
        assert_eq!(steps[2].data.as_slice(), &[1]);
    }

    #[test]
    fn write_chunks_split_into_8_byte_frames() {
        let mut writer = FlashWriter::new(20, 10);
        let firmware = [
            1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
        ];
        let mut remaining = &firmware[..];

        let c1 = writer.next_chunk(&mut remaining).unwrap();
        assert_eq!(c1.bytes.as_slice(), &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(c1.wait, Duration::from_millis(10));
        assert!(c1.extra_wait_after.is_none());

        let c2 = writer.next_chunk(&mut remaining).unwrap();
        assert_eq!(c2.bytes.as_slice(), &[9, 10, 11, 12, 13, 14, 15, 16]);

        let c3 = writer.next_chunk(&mut remaining).unwrap();
        assert_eq!(c3.bytes.as_slice(), &[17, 18, 19, 20]);

        assert!(writer.next_chunk(&mut remaining).is_none());
        assert_eq!(writer.remaining_filesize(), 0);
    }

    #[test]
    fn write_chunk_clamps_to_declared_filesize_even_with_more_input() {
        let mut writer = FlashWriter::new(3, 10);
        let firmware = [1u8, 2, 3, 4, 5, 6, 7, 8];
        let mut remaining = &firmware[..];

        let chunk = writer.next_chunk(&mut remaining).unwrap();
        assert_eq!(chunk.bytes.as_slice(), &[1, 2, 3]);
        assert!(writer.next_chunk(&mut remaining).is_none());
    }

    #[test]
    fn extra_pause_fires_exactly_on_4096_byte_boundary() {
        let mut writer = FlashWriter::new(4096, 0);
        let firmware = [0u8; 4096];
        let mut remaining = &firmware[..];

        let mut saw_pause_at = None;
        let mut sent = 0usize;
        let mut chunk_index = 0;
        while let Some(chunk) = writer.next_chunk(&mut remaining) {
            sent += chunk.bytes.len();
            if chunk.extra_wait_after.is_some() {
                saw_pause_at = Some((chunk_index, sent));
            }
            chunk_index += 1;
        }
        // 4096 / 8 = 512 chunks; the pause fires only after the very last one.
        assert_eq!(saw_pause_at, Some((511, 4096)));
    }
}
