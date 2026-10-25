//! The bus master's side of the block firmware update ([`crate::ota`]), as far
//! as it can be kept apart from the bus: what one block needs — its header, its
//! frames, and the decision after every answer (or lack of one) whether it
//! is through, is sent again, or the update has failed. The caller sends the
//! frames, waits for [`BLOCK_TIMEOUT_MS`](crate::ota::BLOCK_TIMEOUT_MS) for an
//! answer and calls [`BlockSender::answer`] with it.
//!
//! ```text
//! let mut block = BlockSender::new(index, data);
//! loop {
//!     send(block.header()); for frame in ota::frames(data) { send_data(frame); }
//!     loop {
//!         match block.answer(wait_for_reply(timeout)) {
//!             Verdict::Wait => continue,        // an answer that is not for this block
//!             Verdict::Acked => break 'blocks,
//!             Verdict::Retry => continue 'send, // send the block again
//!             Verdict::Failed(why) => return Err(why),
//!         }
//!     }
//! }
//! ```

use crate::ota::{crc16, NakReason, OtaControl, OtaErrorCode, MAX_RETRIES};

/// What to do after an answer (or the lack of one).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The block is safe on the node; go on with the next.
    Acked,
    /// Send the block (header and frames) again.
    Retry,
    /// That was not about this block: keep waiting.
    Wait,
    /// The update has failed.
    Failed(Failure),
}

/// Why an update failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// A block was refused or not answered [`MAX_RETRIES`] + 1 times.
    TooManyRetries,
    /// The node ended the update.
    Node(OtaErrorCode),
    /// The node got the blocks out of order: the sender and the node disagree
    /// about where the update is.
    OutOfOrder,
}

/// One block on its way.
#[derive(Debug, Clone, Copy)]
pub struct BlockSender {
    index: u16,
    len: u16,
    crc16: u16,
    retries: u8,
}

impl BlockSender {
    /// For block `index` with these bytes (at most 65535 of them).
    pub fn new(index: u16, data: &[u8]) -> Self {
        Self {
            index,
            len: data.len() as u16,
            crc16: crc16(data),
            retries: 0,
        }
    }

    /// The header frame to send before the data frames.
    pub fn header(&self) -> OtaControl {
        OtaControl::Block {
            index: self.index,
            len: self.len,
            crc16: self.crc16,
        }
    }

    /// How often the block had to be sent again so far.
    pub fn retries(&self) -> u8 {
        self.retries
    }

    fn retry(&mut self) -> Verdict {
        if self.retries >= MAX_RETRIES {
            return Verdict::Failed(Failure::TooManyRetries);
        }
        self.retries += 1;
        Verdict::Retry
    }

    /// The node's answer, or `None` if none came in time.
    pub fn answer(&mut self, reply: Option<OtaControl>) -> Verdict {
        match reply {
            None => self.retry(),
            Some(OtaControl::Ack { next }) if next == self.index.wrapping_add(1) => Verdict::Acked,
            Some(OtaControl::Nak {
                index,
                reason: NakReason::OutOfOrder,
            }) if index != self.index => Verdict::Failed(Failure::OutOfOrder),
            Some(OtaControl::Nak { index, .. }) if index == self.index => self.retry(),
            Some(OtaControl::Error(code)) => Verdict::Failed(Failure::Node(code)),
            // Anything else is a late answer to an earlier block or request.
            Some(_) => Verdict::Wait,
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    fn block() -> BlockSender {
        BlockSender::new(3, &[1, 2, 3, 4, 5])
    }

    #[test]
    fn the_header_describes_the_block() {
        let data = [9u8, 8, 7];
        let sender = BlockSender::new(12, &data);
        assert_eq!(
            sender.header(),
            OtaControl::Block {
                index: 12,
                len: 3,
                crc16: crc16(&data)
            }
        );
    }

    #[test]
    fn an_ack_for_the_next_block_finishes_it() {
        let mut b = block();
        assert_eq!(b.answer(Some(OtaControl::Ack { next: 4 })), Verdict::Acked);
    }

    #[test]
    fn stale_answers_are_waited_out() {
        let mut b = block();
        assert_eq!(b.answer(Some(OtaControl::Ack { next: 3 })), Verdict::Wait);
        assert_eq!(b.answer(Some(OtaControl::Ack { next: 9 })), Verdict::Wait);
        assert_eq!(
            b.answer(Some(OtaControl::Nak {
                index: 2,
                reason: NakReason::BadCrc
            })),
            Verdict::Wait
        );
        assert_eq!(b.answer(Some(OtaControl::Done)), Verdict::Wait);
        assert_eq!(b.retries(), 0);
    }

    #[test]
    fn a_nak_or_silence_repeats_the_block_up_to_the_limit() {
        let mut b = block();
        for _ in 0..MAX_RETRIES {
            assert_eq!(
                b.answer(Some(OtaControl::Nak {
                    index: 3,
                    reason: NakReason::Short
                })),
                Verdict::Retry
            );
        }
        assert_eq!(b.retries(), MAX_RETRIES);
        assert_eq!(b.answer(None), Verdict::Failed(Failure::TooManyRetries));
    }

    #[test]
    fn a_timeout_counts_like_a_nak() {
        let mut b = block();
        assert_eq!(b.answer(None), Verdict::Retry);
        assert_eq!(b.answer(Some(OtaControl::Ack { next: 4 })), Verdict::Acked);
        assert_eq!(b.retries(), 1);
    }

    #[test]
    fn errors_and_a_confused_node_end_the_update() {
        let mut b = block();
        assert_eq!(
            b.answer(Some(OtaControl::Error(OtaErrorCode::FlashFailed))),
            Verdict::Failed(Failure::Node(OtaErrorCode::FlashFailed))
        );
        let mut b = block();
        assert_eq!(
            b.answer(Some(OtaControl::Nak {
                index: 1,
                reason: NakReason::OutOfOrder
            })),
            Verdict::Failed(Failure::OutOfOrder)
        );
    }
}
