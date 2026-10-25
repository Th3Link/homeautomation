//! The node's side of the block firmware update ([`crate::ota`]): a state
//! machine that takes the control and data frames and says what to do about
//! them. It does no I/O and has no clock: the caller passes the time in
//! milliseconds, carries out what comes back (answer a frame, write a sector,
//! verify the image) and reports the result.
//!
//! Blocks are collected in a flash-sector sized buffer and written when it is
//! full (or the image ends). A block is acknowledged when it is CRC-checked
//! and safe in the buffer; the block that completes a sector is acknowledged
//! after the sector is written, so a received ack always means "this is not
//! going to be lost". A repeated block (its ack was lost) is acknowledged
//! again and dropped.

use crate::ota::{
    crc16, supported_block_size, NakReason, OtaControl, OtaErrorCode, SECTOR_SIZE, STALL_MS,
};

/// What the caller has to do next.
#[derive(Debug, PartialEq, Eq)]
pub enum Effect<'a> {
    /// Nothing.
    None,
    /// Send this control frame.
    Reply(OtaControl),
    /// Start writing an image of `size` bytes with this CRC-32 to the update
    /// partition; then call [`OtaReceiver::begun`].
    Begin { size: u32, crc32: u32 },
    /// Write `data` at byte `offset` of the image; then call
    /// [`OtaReceiver::sector_written`].
    WriteSector { offset: u32, data: &'a [u8] },
    /// Verify the whole image and, if it is good, make it the one to boot;
    /// then call [`OtaReceiver::verified`].
    Verify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    Beginning,
    Ready,
    Receiving,
    Flushing,
    Verifying,
    Failed,
}

#[derive(Debug, Clone, Copy)]
struct Block {
    index: u16,
    len: usize,
    crc16: u16,
    got: usize,
    last_frame_ms: u64,
}

/// See the module docs.
pub struct OtaReceiver {
    state: State,
    size: u32,
    block_size: usize,
    sector: [u8; SECTOR_SIZE],
    /// Bytes of the sector buffer in use.
    fill: usize,
    /// Where in the image the buffered sector starts.
    sector_offset: u32,
    next_block: u16,
    block: Block,
}

impl Default for OtaReceiver {
    fn default() -> Self {
        Self::new()
    }
}

impl OtaReceiver {
    pub const fn new() -> Self {
        Self {
            state: State::Idle,
            size: 0,
            block_size: 0,
            sector: [0; SECTOR_SIZE],
            fill: 0,
            sector_offset: 0,
            next_block: 0,
            block: Block {
                index: 0,
                len: 0,
                crc16: 0,
                got: 0,
                last_frame_ms: 0,
            },
        }
    }

    /// Whether an update is under way (between `Begin` and its end).
    pub fn active(&self) -> bool {
        !matches!(self.state, State::Idle | State::Failed)
    }

    /// Bytes of the image that are in (flash or the sector buffer).
    pub fn received(&self) -> u32 {
        self.sector_offset + self.fill as u32
    }

    fn reply(control: OtaControl) -> Effect<'static> {
        Effect::Reply(control)
    }

    fn error(&mut self, code: OtaErrorCode) -> Effect<'static> {
        self.state = State::Failed;
        Self::reply(OtaControl::Error(code))
    }

    /// A control frame from the bus master.
    pub fn control(&mut self, frame: OtaControl, now_ms: u64) -> Effect<'_> {
        match frame {
            OtaControl::Begin {
                block_size,
                size,
                crc32,
            } => {
                let block_size = usize::from(block_size);
                if size == 0 || !supported_block_size(block_size) {
                    return self.error(OtaErrorCode::BadBegin);
                }
                // A new Begin starts over, whatever was going on.
                self.state = State::Beginning;
                self.size = size;
                self.block_size = block_size;
                self.fill = 0;
                self.sector_offset = 0;
                self.next_block = 0;
                Effect::Begin { size, crc32 }
            }
            OtaControl::Block { index, len, crc16 } => self.block_header(index, len, crc16, now_ms),
            OtaControl::End => match self.state {
                State::Ready if self.received() == self.size => {
                    self.state = State::Verifying;
                    Effect::Verify
                }
                State::Ready | State::Receiving => self.error(OtaErrorCode::Incomplete),
                State::Flushing | State::Verifying | State::Beginning => Effect::None,
                State::Idle | State::Failed => self.error(OtaErrorCode::NotStarted),
            },
            OtaControl::Abort => {
                self.state = State::Idle;
                Effect::None
            }
            // What a node sends, not what it takes.
            OtaControl::Ack { .. }
            | OtaControl::Nak { .. }
            | OtaControl::Done
            | OtaControl::Error(_) => Effect::None,
        }
    }

    fn block_header(&mut self, index: u16, len: u16, crc16: u16, now_ms: u64) -> Effect<'_> {
        match self.state {
            State::Ready | State::Receiving => {}
            State::Flushing | State::Verifying | State::Beginning => return Effect::None,
            State::Idle | State::Failed => return self.error(OtaErrorCode::NotStarted),
        }
        if index < self.next_block {
            // Its ack got lost: say so again; its data frames fall on a
            // receiver that is not taking a block and are ignored.
            self.state = State::Ready;
            return Self::reply(OtaControl::Ack {
                next: self.next_block,
            });
        }
        if index > self.next_block {
            return Self::reply(OtaControl::Nak {
                index: self.next_block,
                reason: NakReason::OutOfOrder,
            });
        }
        let len = usize::from(len);
        let remaining = (self.size - self.received()) as usize;
        if len == 0 || len != self.block_size.min(remaining) {
            self.state = State::Ready;
            return Self::reply(OtaControl::Nak {
                index,
                reason: NakReason::BadHeader,
            });
        }
        // The block begins (again, if the sender repeats it).
        self.state = State::Receiving;
        self.block = Block {
            index,
            len,
            crc16,
            got: 0,
            last_frame_ms: now_ms,
        };
        Effect::None
    }

    /// The bytes of one `OtaData` frame.
    pub fn data(&mut self, bytes: &[u8], now_ms: u64) -> Effect<'_> {
        if self.state != State::Receiving {
            return Effect::None;
        }
        let take = bytes.len().min(self.block.len - self.block.got);
        let start = self.fill + self.block.got;
        self.sector[start..start + take].copy_from_slice(&bytes[..take]);
        self.block.got += take;
        self.block.last_frame_ms = now_ms;
        if self.block.got < self.block.len {
            return Effect::None;
        }

        let data = &self.sector[self.fill..self.fill + self.block.len];
        if crc16(data) != self.block.crc16 {
            self.state = State::Ready;
            return Self::reply(OtaControl::Nak {
                index: self.block.index,
                reason: NakReason::BadCrc,
            });
        }
        self.fill += self.block.len;
        self.next_block = self.block.index + 1;
        let image_done = self.received() == self.size;
        if self.fill == SECTOR_SIZE || image_done {
            self.state = State::Flushing;
            return Effect::WriteSector {
                offset: self.sector_offset,
                data: &self.sector[..self.fill],
            };
        }
        self.state = State::Ready;
        Self::reply(OtaControl::Ack {
            next: self.next_block,
        })
    }

    /// Call now and then (every few tens of ms is plenty): a block whose
    /// frames stopped coming is asked for again.
    pub fn tick(&mut self, now_ms: u64) -> Effect<'_> {
        if self.state == State::Receiving
            && now_ms.saturating_sub(self.block.last_frame_ms) >= u64::from(STALL_MS)
        {
            self.state = State::Ready;
            return Self::reply(OtaControl::Nak {
                index: self.block.index,
                reason: NakReason::Short,
            });
        }
        Effect::None
    }

    /// The result of [`Effect::Begin`].
    pub fn begun(&mut self, ok: bool) -> Effect<'_> {
        if self.state != State::Beginning {
            return Effect::None;
        }
        if ok {
            self.state = State::Ready;
            Self::reply(OtaControl::Ack { next: 0 })
        } else {
            self.error(OtaErrorCode::TooLarge)
        }
    }

    /// The result of [`Effect::WriteSector`].
    pub fn sector_written(&mut self, ok: bool) -> Effect<'_> {
        if self.state != State::Flushing {
            return Effect::None;
        }
        if !ok {
            return self.error(OtaErrorCode::FlashFailed);
        }
        self.sector_offset += self.fill as u32;
        self.fill = 0;
        self.state = State::Ready;
        Self::reply(OtaControl::Ack {
            next: self.next_block,
        })
    }

    /// The result of [`Effect::Verify`]. On success the node should restart
    /// shortly after the `Done` has gone out.
    pub fn verified(&mut self, ok: bool) -> Effect<'_> {
        if self.state != State::Verifying {
            return Effect::None;
        }
        if ok {
            self.state = State::Idle;
            Self::reply(OtaControl::Done)
        } else {
            self.error(OtaErrorCode::VerifyFailed)
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use crate::ota::{frames, BLOCK_SIZE};

    fn image(len: usize) -> std::vec::Vec<u8> {
        (0..len).map(|i| (i * 7 + i / 251) as u8).collect()
    }

    fn begin(size: u32, block_size: u16) -> OtaControl {
        OtaControl::Begin {
            block_size,
            size,
            crc32: 0,
        }
    }

    /// A receiver that has begun an update of `size` bytes.
    fn started(size: u32) -> OtaReceiver {
        let mut r = OtaReceiver::new();
        assert!(matches!(
            r.control(begin(size, BLOCK_SIZE as u16), 0),
            Effect::Begin { .. }
        ));
        assert_eq!(r.begun(true), Effect::Reply(OtaControl::Ack { next: 0 }));
        r
    }

    fn send_block<'a>(r: &'a mut OtaReceiver, index: u16, data: &[u8], now: u64) -> Effect<'a> {
        let header = OtaControl::Block {
            index,
            len: data.len() as u16,
            crc16: crc16(data),
        };
        assert_eq!(r.control(header, now), Effect::None);
        let mut last = Effect::None;
        let frames: std::vec::Vec<_> = frames(data).collect();
        let count = frames.len();
        for (i, frame) in frames.iter().enumerate() {
            if i + 1 == count {
                return r.data(frame, now);
            }
            assert_eq!(r.data(frame, now), Effect::None);
            last = Effect::None;
        }
        last
    }

    #[test]
    fn begin_needs_a_sane_block_size_and_size() {
        let mut r = OtaReceiver::new();
        assert_eq!(
            r.control(begin(0, 1024), 0),
            Effect::Reply(OtaControl::Error(OtaErrorCode::BadBegin))
        );
        // 8192 cannot even be sent; a block size that does not divide a
        // sector is refused.
        assert_eq!(
            r.control(begin(100, 768), 0),
            Effect::Reply(OtaControl::Error(OtaErrorCode::BadBegin))
        );
        assert!(!r.active());
    }

    #[test]
    fn the_partition_can_be_too_small() {
        let mut r = OtaReceiver::new();
        assert_eq!(
            r.control(begin(5000, 1024), 0),
            Effect::Begin {
                size: 5000,
                crc32: 0
            }
        );
        assert_eq!(
            r.begun(false),
            Effect::Reply(OtaControl::Error(OtaErrorCode::TooLarge))
        );
        assert!(!r.active());
    }

    #[test]
    fn blocks_are_acked_and_sectors_written_when_full() {
        let data = image(2 * SECTOR_SIZE);
        let mut r = started(data.len() as u32);
        let blocks_per_sector = SECTOR_SIZE / BLOCK_SIZE;
        for i in 0..blocks_per_sector - 1 {
            let chunk = &data[i * BLOCK_SIZE..(i + 1) * BLOCK_SIZE];
            assert_eq!(
                send_block(&mut r, i as u16, chunk, 0),
                Effect::Reply(OtaControl::Ack { next: i as u16 + 1 })
            );
        }
        // The block that fills the sector is acked after the sector is written.
        let i = blocks_per_sector - 1;
        let chunk = &data[i * BLOCK_SIZE..(i + 1) * BLOCK_SIZE];
        let effect = send_block(&mut r, i as u16, chunk, 0);
        let Effect::WriteSector {
            offset,
            data: sector,
        } = effect
        else {
            panic!("expected a sector write");
        };
        assert_eq!(offset, 0);
        assert_eq!(sector, &data[..SECTOR_SIZE]);
        assert_eq!(
            r.sector_written(true),
            Effect::Reply(OtaControl::Ack {
                next: blocks_per_sector as u16
            })
        );
    }

    #[test]
    fn a_short_last_block_and_end() {
        let data = image(BLOCK_SIZE + 100);
        let mut r = started(data.len() as u32);
        assert_eq!(
            send_block(&mut r, 0, &data[..BLOCK_SIZE], 0),
            Effect::Reply(OtaControl::Ack { next: 1 })
        );
        // The image ends with this block: it is written out right away.
        let effect = send_block(&mut r, 1, &data[BLOCK_SIZE..], 0);
        assert_eq!(
            effect,
            Effect::WriteSector {
                offset: 0,
                data: &data[..]
            }
        );
        assert_eq!(
            r.sector_written(true),
            Effect::Reply(OtaControl::Ack { next: 2 })
        );
        assert_eq!(r.control(OtaControl::End, 0), Effect::Verify);
        assert_eq!(r.verified(true), Effect::Reply(OtaControl::Done));
        assert!(!r.active());
    }

    #[test]
    fn a_missing_frame_gives_a_short_nak_after_the_stall_time() {
        let data = image(BLOCK_SIZE * 2);
        let mut r = started(data.len() as u32);
        let block = &data[..BLOCK_SIZE];
        let header = OtaControl::Block {
            index: 0,
            len: BLOCK_SIZE as u16,
            crc16: crc16(block),
        };
        r.control(header, 100);
        for (i, frame) in frames(block).enumerate() {
            if i != 17 {
                assert_eq!(r.data(&frame, 100 + i as u64), Effect::None);
            }
        }
        assert_eq!(r.tick(150), Effect::None);
        assert_eq!(
            r.tick(100 + 128 + u64::from(STALL_MS)),
            Effect::Reply(OtaControl::Nak {
                index: 0,
                reason: NakReason::Short
            })
        );
        // The sender repeats the block: now it goes through.
        assert_eq!(
            send_block(&mut r, 0, block, 1000),
            Effect::Reply(OtaControl::Ack { next: 1 })
        );
    }

    #[test]
    fn a_wrong_crc_gives_a_nak_and_leaves_no_trace() {
        let data = image(BLOCK_SIZE * 2);
        let mut r = started(data.len() as u32);
        let block = &data[..BLOCK_SIZE];
        let header = OtaControl::Block {
            index: 0,
            len: BLOCK_SIZE as u16,
            crc16: crc16(block) ^ 1,
        };
        r.control(header, 0);
        let mut effect = Effect::None;
        for frame in frames(block) {
            effect = r.data(&frame, 0);
        }
        assert_eq!(
            effect,
            Effect::Reply(OtaControl::Nak {
                index: 0,
                reason: NakReason::BadCrc
            })
        );
        assert_eq!(r.received(), 0);
        assert_eq!(
            send_block(&mut r, 0, block, 0),
            Effect::Reply(OtaControl::Ack { next: 1 })
        );
    }

    #[test]
    fn a_repeated_block_is_acked_again_and_not_taken_twice() {
        let data = image(BLOCK_SIZE * 3);
        let mut r = started(data.len() as u32);
        send_block(&mut r, 0, &data[..BLOCK_SIZE], 0);
        let received = r.received();
        // The sender missed the ack and sends block 0 again.
        let header = OtaControl::Block {
            index: 0,
            len: BLOCK_SIZE as u16,
            crc16: crc16(&data[..BLOCK_SIZE]),
        };
        assert_eq!(
            r.control(header, 0),
            Effect::Reply(OtaControl::Ack { next: 1 })
        );
        for frame in frames(&data[..BLOCK_SIZE]) {
            assert_eq!(r.data(&frame, 0), Effect::None);
        }
        assert_eq!(r.received(), received);
        assert_eq!(
            send_block(&mut r, 1, &data[BLOCK_SIZE..2 * BLOCK_SIZE], 0),
            Effect::Reply(OtaControl::Ack { next: 2 })
        );
    }

    #[test]
    fn a_block_ahead_is_refused_with_the_expected_index() {
        let data = image(BLOCK_SIZE * 3);
        let mut r = started(data.len() as u32);
        let header = OtaControl::Block {
            index: 2,
            len: BLOCK_SIZE as u16,
            crc16: 0,
        };
        assert_eq!(
            r.control(header, 0),
            Effect::Reply(OtaControl::Nak {
                index: 0,
                reason: NakReason::OutOfOrder
            })
        );
    }

    #[test]
    fn a_header_that_does_not_fit_is_refused() {
        let data = image(BLOCK_SIZE + 100);
        let mut r = started(data.len() as u32);
        for len in [0u16, 100, (BLOCK_SIZE + 1) as u16] {
            let header = OtaControl::Block {
                index: 0,
                len,
                crc16: 0,
            };
            assert_eq!(
                r.control(header, 0),
                Effect::Reply(OtaControl::Nak {
                    index: 0,
                    reason: NakReason::BadHeader
                }),
                "{len}"
            );
        }
    }

    #[test]
    fn a_repeated_header_restarts_the_block() {
        let data = image(BLOCK_SIZE * 2);
        let mut r = started(data.len() as u32);
        let block = &data[..BLOCK_SIZE];
        let header = OtaControl::Block {
            index: 0,
            len: BLOCK_SIZE as u16,
            crc16: crc16(block),
        };
        r.control(header, 0);
        for frame in frames(block).take(10) {
            r.data(&frame, 0);
        }
        // The sender gave up on it and starts the block over.
        assert_eq!(
            send_block(&mut r, 0, block, 5),
            Effect::Reply(OtaControl::Ack { next: 1 })
        );
    }

    #[test]
    fn failures_are_reported_and_stop_the_update() {
        let data = image(SECTOR_SIZE);
        let mut r = started(data.len() as u32);
        for i in 0..SECTOR_SIZE / BLOCK_SIZE - 1 {
            send_block(
                &mut r,
                i as u16,
                &data[i * BLOCK_SIZE..(i + 1) * BLOCK_SIZE],
                0,
            );
        }
        let last = SECTOR_SIZE / BLOCK_SIZE - 1;
        assert!(matches!(
            send_block(&mut r, last as u16, &data[last * BLOCK_SIZE..], 0),
            Effect::WriteSector { .. }
        ));
        assert_eq!(
            r.sector_written(false),
            Effect::Reply(OtaControl::Error(OtaErrorCode::FlashFailed))
        );
        assert!(!r.active());
        // Nothing goes on after that.
        assert_eq!(
            r.control(OtaControl::End, 0),
            Effect::Reply(OtaControl::Error(OtaErrorCode::NotStarted))
        );
    }

    #[test]
    fn end_before_the_last_block_is_an_error_and_verify_can_fail() {
        let data = image(BLOCK_SIZE * 2);
        let mut r = started(data.len() as u32);
        send_block(&mut r, 0, &data[..BLOCK_SIZE], 0);
        assert_eq!(
            r.control(OtaControl::End, 0),
            Effect::Reply(OtaControl::Error(OtaErrorCode::Incomplete))
        );

        let mut r = started(BLOCK_SIZE as u32);
        assert!(matches!(
            send_block(&mut r, 0, &data[..BLOCK_SIZE], 0),
            Effect::WriteSector { .. }
        ));
        r.sector_written(true);
        assert_eq!(r.control(OtaControl::End, 0), Effect::Verify);
        assert_eq!(
            r.verified(false),
            Effect::Reply(OtaControl::Error(OtaErrorCode::VerifyFailed))
        );
    }

    #[test]
    fn frames_and_blocks_without_an_update_are_ignored_or_refused() {
        let mut r = OtaReceiver::new();
        assert_eq!(r.data(&[0; 8], 0), Effect::None);
        assert_eq!(
            r.control(
                OtaControl::Block {
                    index: 0,
                    len: 8,
                    crc16: 0
                },
                0
            ),
            Effect::Reply(OtaControl::Error(OtaErrorCode::NotStarted))
        );
        assert_eq!(r.tick(10_000), Effect::None);
        assert_eq!(r.control(OtaControl::Abort, 0), Effect::None);
    }

    #[test]
    fn abort_ends_the_update_and_a_new_begin_starts_over() {
        let data = image(BLOCK_SIZE * 2);
        let mut r = started(data.len() as u32);
        send_block(&mut r, 0, &data[..BLOCK_SIZE], 0);
        r.control(OtaControl::Abort, 0);
        assert!(!r.active());
        assert!(matches!(
            r.control(begin(data.len() as u32, BLOCK_SIZE as u16), 0),
            Effect::Begin { .. }
        ));
        assert_eq!(r.begun(true), Effect::Reply(OtaControl::Ack { next: 0 }));
        assert_eq!(r.received(), 0);
    }
}
