//! Reading a node's log over CAN (`LogDownload`, ADR 0026): the node keeps
//! its latest log lines in a [`LogRing`]; asked by a request (R) frame, it
//! sends them in blocks of whole lines, and drops a block from the ring only
//! once the reader acknowledged it, so that no line is lost and none comes
//! twice. The node's side is [`LogSender`], the reader's [`LogReceiver`].
//!
//! Every frame is a `LogDownload` (155) frame with the node's id; the top two
//! bits of byte 0 say what it is:
//!
//! | op | frame | direction | layout |
//! |---|---|---|---|
//! | – | request (R frame) | to the node | send the log (again from the unacknowledged block) |
//! | 0 | [`LogFrame::Data`] | from the node | byte 0: frame index in the block (0-31); bytes 1-7: log bytes (the last frame shorter) |
//! | 1 | [`LogFrame::BlockEnd`] | from the node | byte 0: data frames; byte 1: block number; bytes 2-3: length; bytes 4-5: CRC-16; bytes 6-7: bytes still waiting |
//! | 2 | [`LogFrame::Ack`] | to the node | byte 1: the block number |
//! | 3 | [`LogFrame::End`] | from the node | byte 1: the next block number; bytes 2-3: bytes overwritten before they were read |
//!
//! Numbers are little-endian; the CRC is [`crate::ota::crc16`]. A block the
//! reader did not get whole is simply not acknowledged: the node sends it
//! again after [`ACK_TIMEOUT_MS`], [`MAX_RETRIES`] times, then stops and
//! keeps it for the next request. A block whose ack was lost comes again
//! with the same number; the reader acknowledges it again and drops it.

use crate::ota::crc16;
use crate::{DecodeError, Payload};
use core::fmt::Write;

/// Log bytes in a data frame (byte 0 is the header).
pub const FRAME_DATA: usize = 7;
/// Data frames in a block at most.
pub const BLOCK_FRAMES: usize = 32;
/// Log bytes in a block at most.
pub const BLOCK_BYTES: usize = FRAME_DATA * BLOCK_FRAMES;
/// How long the node waits for the ack of a block before it sends it again.
pub const ACK_TIMEOUT_MS: u64 = 500;
/// How often the node sends a block again before it gives up.
pub const MAX_RETRIES: u8 = 5;

const OP_DATA: u8 = 0;
const OP_BLOCK_END: u8 = 1;
const OP_ACK: u8 = 2;
const OP_END: u8 = 3;

/// A `LogDownload` data frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFrame {
    /// Up to [`FRAME_DATA`] log bytes, the `index`th frame of a block.
    Data {
        index: u8,
        bytes: [u8; FRAME_DATA],
        len: u8,
    },
    /// The end of block `seq`: `frames` data frames with `len` bytes.
    BlockEnd {
        frames: u8,
        seq: u8,
        len: u16,
        crc: u16,
        /// Log bytes the node still has after this block.
        waiting: u16,
    },
    /// Block `seq` arrived whole.
    Ack { seq: u8 },
    /// Nothing more to send; `lost` bytes were overwritten unread since the
    /// last download.
    End { seq: u8, lost: u16 },
}

impl LogFrame {
    pub fn to_payload(&self) -> Payload {
        let mut b = [0u8; 8];
        let len = match *self {
            Self::Data { index, bytes, len } => {
                b[0] = OP_DATA << 6 | (index & 0x3F);
                let len = usize::from(len).min(FRAME_DATA);
                b[1..=len].copy_from_slice(&bytes[..len]);
                1 + len
            }
            Self::BlockEnd {
                frames,
                seq,
                len,
                crc,
                waiting,
            } => {
                b[0] = OP_BLOCK_END << 6 | (frames & 0x3F);
                b[1] = seq;
                b[2..4].copy_from_slice(&len.to_le_bytes());
                b[4..6].copy_from_slice(&crc.to_le_bytes());
                b[6..8].copy_from_slice(&waiting.to_le_bytes());
                8
            }
            Self::Ack { seq } => {
                b[0] = OP_ACK << 6;
                b[1] = seq;
                2
            }
            Self::End { seq, lost } => {
                b[0] = OP_END << 6;
                b[1] = seq;
                b[2..4].copy_from_slice(&lost.to_le_bytes());
                4
            }
        };
        Payload::new(b, len)
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, DecodeError> {
        let (&head, rest) = data.split_first().ok_or(DecodeError)?;
        let u16_at = |i: usize| -> Result<u16, DecodeError> {
            Ok(u16::from_le_bytes([
                *rest.get(i).ok_or(DecodeError)?,
                *rest.get(i + 1).ok_or(DecodeError)?,
            ]))
        };
        match head >> 6 {
            OP_DATA => {
                if rest.is_empty() || rest.len() > FRAME_DATA {
                    return Err(DecodeError);
                }
                let mut bytes = [0u8; FRAME_DATA];
                bytes[..rest.len()].copy_from_slice(rest);
                Ok(Self::Data {
                    index: head & 0x3F,
                    bytes,
                    len: rest.len() as u8,
                })
            }
            OP_BLOCK_END => Ok(Self::BlockEnd {
                frames: head & 0x3F,
                seq: *rest.first().ok_or(DecodeError)?,
                len: u16_at(1)?,
                crc: u16_at(3)?,
                waiting: u16_at(5)?,
            }),
            OP_ACK => Ok(Self::Ack {
                seq: *rest.first().ok_or(DecodeError)?,
            }),
            _ => Ok(Self::End {
                seq: *rest.first().ok_or(DecodeError)?,
                lost: u16_at(1)?,
            }),
        }
    }
}

fn saturate(n: u32) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

/// The node's latest log lines, `N` bytes, the oldest overwritten. Lines go
/// in whole ([`Self::push_line`]); a download takes them out
/// ([`Self::next_block`], [`Self::consume_to`]). Positions count the bytes
/// logged since boot.
pub struct LogRing<const N: usize> {
    buf: [u8; N],
    /// Bytes logged since boot; `written % N` is where the next one goes.
    written: u32,
    /// Where the bytes not yet downloaded start.
    read: u32,
    /// Bytes overwritten before they were downloaded.
    lost: u32,
}

impl<const N: usize> Default for LogRing<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> LogRing<N> {
    pub const fn new() -> Self {
        Self {
            buf: [0; N],
            written: 0,
            read: 0,
            lost: 0,
        }
    }

    fn push(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.buf[self.written as usize % N] = b;
            self.written = self.written.wrapping_add(1);
        }
        let oldest = self.written.saturating_sub(N as u32);
        if self.read < oldest {
            self.lost = self.lost.saturating_add(oldest - self.read);
            self.read = oldest;
        }
    }

    /// Adds a line, `[uptime] text\n` like the gateway's log.
    pub fn push_line(&mut self, uptime_ms: u64, line: core::fmt::Arguments<'_>) {
        struct Into<'r, const N: usize>(&'r mut LogRing<N>);
        impl<const N: usize> Write for Into<'_, N> {
            fn write_str(&mut self, s: &str) -> core::fmt::Result {
                self.0.push(s.as_bytes());
                Ok(())
            }
        }
        let mut into = Into(self);
        let _ = write!(
            into,
            "[{:>6}.{}] ",
            uptime_ms / 1000,
            uptime_ms % 1000 / 100
        );
        let _ = into.write_fmt(line);
        self.push(b"\n");
    }

    /// Bytes not yet downloaded.
    pub fn waiting(&self) -> u32 {
        self.written - self.read
    }

    /// Copies the next block into `out`: up to [`BLOCK_BYTES`] bytes from
    /// the oldest not downloaded, ending after a line break if there is one
    /// in it (else on a character boundary). Its length and the position
    /// after it; `None` if there is nothing.
    pub fn next_block(&self, out: &mut [u8; BLOCK_BYTES]) -> Option<(usize, u32)> {
        let available = self.waiting().min(BLOCK_BYTES as u32) as usize;
        if available == 0 {
            return None;
        }
        for (i, slot) in out.iter_mut().take(available).enumerate() {
            *slot = self.buf[(self.read as usize + i) % N];
        }
        let mut len = available;
        if (self.waiting() as usize) > available {
            if let Some(newline) = out.iter().rposition(|&b| b == b'\n') {
                len = newline + 1;
            } else {
                // A line longer than a block: don't cut a character in two
                // (the byte after the block mustn't continue one).
                let at = |i: usize| self.buf[(self.read as usize + i) % N];
                while len > 1 && at(len) & 0xC0 == 0x80 {
                    len -= 1;
                }
            }
        }
        Some((len, self.read + len as u32))
    }

    /// Drops what was downloaded, up to position `end`.
    pub fn consume_to(&mut self, end: u32) {
        self.read = self.read.max(end.min(self.written));
    }

    /// The bytes lost since the last call (saturated), and starts counting
    /// again.
    pub fn take_lost(&mut self) -> u16 {
        let lost = saturate(self.lost);
        self.lost = 0;
        lost
    }
}

/// What the node is to do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Send the block [`LogSender::block_frames`] gives.
    SendBlock,
    /// Send this [`LogFrame::End`].
    SendEnd(LogFrame),
    /// Nothing (now).
    Idle,
}

struct Pending {
    seq: u8,
    block: [u8; BLOCK_BYTES],
    len: usize,
    /// The ring position after the block.
    end: u32,
    waiting: u16,
    retries: u8,
    deadline_ms: u64,
}

/// The node's side of a download.
pub struct LogSender {
    /// The number of the next block.
    seq: u8,
    pending: Option<Pending>,
}

impl Default for LogSender {
    fn default() -> Self {
        Self::new()
    }
}

impl LogSender {
    pub const fn new() -> Self {
        Self {
            seq: 0,
            pending: None,
        }
    }

    fn next<const N: usize>(&mut self, ring: &mut LogRing<N>, now_ms: u64) -> Action {
        let mut block = [0u8; BLOCK_BYTES];
        match ring.next_block(&mut block) {
            Some((len, end)) => {
                self.pending = Some(Pending {
                    seq: self.seq,
                    block,
                    len,
                    end,
                    waiting: saturate(ring.waiting() - len as u32),
                    retries: 0,
                    deadline_ms: now_ms + ACK_TIMEOUT_MS,
                });
                Action::SendBlock
            }
            None => {
                self.pending = None;
                Action::SendEnd(LogFrame::End {
                    seq: self.seq,
                    lost: ring.take_lost(),
                })
            }
        }
    }

    /// The reader asked for the log (an R frame): the block still waiting
    /// for its ack again, or the next one.
    pub fn request<const N: usize>(&mut self, ring: &mut LogRing<N>, now_ms: u64) -> Action {
        match &mut self.pending {
            Some(pending) => {
                pending.retries = 0;
                pending.deadline_ms = now_ms + ACK_TIMEOUT_MS;
                Action::SendBlock
            }
            None => self.next(ring, now_ms),
        }
    }

    /// An ack came in: block `seq` is done with, on to the next.
    pub fn ack<const N: usize>(&mut self, seq: u8, ring: &mut LogRing<N>, now_ms: u64) -> Action {
        match &self.pending {
            Some(pending) if pending.seq == seq => {
                ring.consume_to(pending.end);
                self.seq = self.seq.wrapping_add(1);
                self.next(ring, now_ms)
            }
            _ => Action::Idle,
        }
    }

    /// Called at [`Self::deadline_ms`]: the block again, or giving up (the
    /// block stays in the ring for the next request).
    pub fn poll(&mut self, now_ms: u64) -> Action {
        let Some(pending) = &mut self.pending else {
            return Action::Idle;
        };
        if now_ms < pending.deadline_ms {
            return Action::Idle;
        }
        if pending.retries >= MAX_RETRIES {
            self.pending = None;
            return Action::Idle;
        }
        pending.retries += 1;
        pending.deadline_ms = now_ms + ACK_TIMEOUT_MS;
        Action::SendBlock
    }

    /// When [`Self::poll`] wants to be called.
    pub fn deadline_ms(&self) -> Option<u64> {
        self.pending.as_ref().map(|pending| pending.deadline_ms)
    }

    /// The frames of the block waiting for its ack: the data frames and the
    /// block end.
    pub fn block_frames(&self) -> impl Iterator<Item = Payload> + '_ {
        let pending = self.pending.as_ref();
        let block = pending.map_or(&[][..], |p| &p.block[..p.len]);
        let data = block.chunks(FRAME_DATA).enumerate().map(|(i, chunk)| {
            let mut bytes = [0u8; FRAME_DATA];
            bytes[..chunk.len()].copy_from_slice(chunk);
            LogFrame::Data {
                index: i as u8,
                bytes,
                len: chunk.len() as u8,
            }
            .to_payload()
        });
        let end = pending.map(|p| {
            LogFrame::BlockEnd {
                frames: p.len.div_ceil(FRAME_DATA) as u8,
                seq: p.seq,
                len: p.len as u16,
                crc: crc16(&p.block[..p.len]),
                waiting: p.waiting,
            }
            .to_payload()
        });
        data.chain(end)
    }
}

/// What came in, for the reader.
#[derive(Debug, PartialEq, Eq)]
pub enum Received<'a> {
    /// Nothing to do yet.
    Nothing,
    /// A new block: take its bytes and send [`LogFrame::Ack`].
    Block {
        seq: u8,
        data: &'a [u8],
        waiting: u16,
    },
    /// A block that came before (its ack was lost): only ack it again.
    Again { seq: u8 },
    /// A block that didn't arrive whole: wait, the node sends it again.
    Damaged,
    /// The node has nothing more.
    End { lost: u16 },
}

/// The reader's side of a download.
pub struct LogReceiver {
    buf: [u8; BLOCK_BYTES],
    /// Which data frames of the current block arrived (bit per index).
    got: u32,
    /// The last block taken.
    last: Option<u8>,
}

impl Default for LogReceiver {
    fn default() -> Self {
        Self::new()
    }
}

impl LogReceiver {
    pub const fn new() -> Self {
        Self {
            buf: [0; BLOCK_BYTES],
            got: 0,
            last: None,
        }
    }

    /// A frame from the node.
    pub fn frame(&mut self, frame: LogFrame) -> Received<'_> {
        match frame {
            LogFrame::Data { index, bytes, len } => {
                let at = usize::from(index) * FRAME_DATA;
                let len = usize::from(len);
                if usize::from(index) < BLOCK_FRAMES && len <= FRAME_DATA {
                    self.buf[at..at + len].copy_from_slice(&bytes[..len]);
                    self.got |= 1 << index;
                }
                Received::Nothing
            }
            LogFrame::BlockEnd {
                frames,
                seq,
                len,
                crc,
                waiting,
            } => {
                let got = core::mem::take(&mut self.got);
                if self.last == Some(seq) {
                    return Received::Again { seq };
                }
                let len = usize::from(len);
                let frames = u32::from(frames);
                let whole = frames as usize == len.div_ceil(FRAME_DATA)
                    && len <= BLOCK_BYTES
                    && frames <= BLOCK_FRAMES as u32
                    && got == u32::MAX >> (32 - frames.max(1))
                    && crc16(&self.buf[..len]) == crc;
                if !whole || len == 0 {
                    return Received::Damaged;
                }
                self.last = Some(seq);
                Received::Block {
                    seq,
                    data: &self.buf[..len],
                    waiting,
                }
            }
            LogFrame::End { lost, .. } => Received::End { lost },
            LogFrame::Ack { .. } => Received::Nothing,
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::string::String;
    use std::vec::Vec;

    fn log<const N: usize>(ring: &mut LogRing<N>, lines: usize) {
        for i in 0..lines {
            ring.push_line(1234 + i as u64, format_args!("line {i}"));
        }
    }

    #[test]
    fn frames_round_trip() {
        for frame in [
            LogFrame::Data {
                index: 31,
                bytes: *b"abcdefg",
                len: 7,
            },
            LogFrame::Data {
                index: 0,
                bytes: *b"x\0\0\0\0\0\0",
                len: 1,
            },
            LogFrame::BlockEnd {
                frames: 32,
                seq: 200,
                len: 224,
                crc: 0xBEEF,
                waiting: 4000,
            },
            LogFrame::Ack { seq: 7 },
            LogFrame::End { seq: 9, lost: 513 },
        ] {
            let payload = frame.to_payload();
            assert_eq!(LogFrame::from_bytes(payload.as_slice()), Ok(frame));
        }
        assert!(LogFrame::from_bytes(&[]).is_err());
        assert!(LogFrame::from_bytes(&[0]).is_err());
        assert!(LogFrame::from_bytes(&[OP_BLOCK_END << 6, 1, 2]).is_err());
    }

    #[test]
    fn lines_carry_the_uptime() {
        let mut ring = LogRing::<256>::new();
        ring.push_line(61_234, format_args!("hello {}", 7));
        let mut block = [0u8; BLOCK_BYTES];
        let (len, _) = ring.next_block(&mut block).unwrap();
        assert_eq!(&block[..len], b"[    61.2] hello 7\n");
    }

    #[test]
    fn blocks_end_after_a_line() {
        let mut ring = LogRing::<4096>::new();
        log(&mut ring, 40);
        let mut block = [0u8; BLOCK_BYTES];
        let (len, end) = ring.next_block(&mut block).unwrap();
        assert!(len <= BLOCK_BYTES && block[len - 1] == b'\n');
        assert_eq!(end, len as u32);
        // A line longer than a block is cut, not stuck.
        let mut ring = LogRing::<4096>::new();
        ring.push_line(0, format_args!("{}", "ä".repeat(200)));
        let (len, _) = ring.next_block(&mut block).unwrap();
        assert!(core::str::from_utf8(&block[..len]).is_ok());
    }

    #[test]
    fn overwritten_lines_are_counted() {
        let mut ring = LogRing::<128>::new();
        log(&mut ring, 20);
        assert_eq!(ring.waiting(), 128);
        assert!(ring.take_lost() > 0);
        assert_eq!(ring.take_lost(), 0);
    }

    /// The node and the reader over a bus that loses the frames `lose`
    /// picks (by the count of frames sent so far).
    fn download<const N: usize>(
        ring: &mut LogRing<N>,
        mut lose: impl FnMut(usize, &LogFrame) -> bool,
    ) -> (String, u16) {
        let mut sender = LogSender::new();
        let mut receiver = LogReceiver::new();
        let mut text = Vec::new();
        let mut now = 0u64;
        let mut sent = 0usize;
        let mut action = sender.request(ring, now);
        for _ in 0..10_000 {
            let frames: Vec<LogFrame> = match action {
                Action::SendBlock => sender
                    .block_frames()
                    .map(|p| LogFrame::from_bytes(p.as_slice()).unwrap())
                    .collect(),
                Action::SendEnd(end) => std::vec![end],
                Action::Idle => Vec::new(),
            };
            action = Action::Idle;
            for frame in frames {
                sent += 1;
                if lose(sent, &frame) {
                    continue;
                }
                match receiver.frame(frame) {
                    Received::Block { seq, data, .. } => {
                        text.extend_from_slice(data);
                        sent += 1;
                        if !lose(sent, &LogFrame::Ack { seq }) {
                            action = sender.ack(seq, ring, now);
                        }
                    }
                    Received::Again { seq } => {
                        sent += 1;
                        if !lose(sent, &LogFrame::Ack { seq }) {
                            action = sender.ack(seq, ring, now);
                        }
                    }
                    Received::End { lost } => return (String::from_utf8(text).unwrap(), lost),
                    Received::Nothing | Received::Damaged => {}
                }
            }
            if action == Action::Idle {
                now = sender.deadline_ms().unwrap_or(now + 1000);
                action = sender.poll(now);
                if action == Action::Idle && sender.deadline_ms().is_none() {
                    // The node gave up: ask again.
                    action = sender.request(ring, now);
                }
            }
        }
        panic!("the download didn't end");
    }

    fn expected(lines: core::ops::Range<usize>) -> String {
        let mut ring = LogRing::<65536>::new();
        let mut text = String::new();
        for i in lines {
            ring.push_line(1234 + i as u64, format_args!("line {i}"));
        }
        let mut block = [0u8; BLOCK_BYTES];
        while let Some((len, end)) = ring.next_block(&mut block) {
            text.push_str(core::str::from_utf8(&block[..len]).unwrap());
            ring.consume_to(end);
        }
        text
    }

    #[test]
    fn a_download_takes_everything_once_and_empties_the_ring() {
        let mut ring = LogRing::<4096>::new();
        log(&mut ring, 100);
        let (text, lost) = download(&mut ring, |_, _| false);
        assert_eq!(text, expected(0..100));
        assert_eq!(lost, 0);
        assert_eq!(ring.waiting(), 0);
        // The next download has only what came since.
        ring.push_line(9_000, format_args!("later"));
        let (text, _) = download(&mut ring, |_, _| false);
        assert_eq!(text, "[     9.0] later\n");
    }

    #[test]
    fn lost_frames_and_acks_lose_and_double_nothing() {
        let mut ring = LogRing::<4096>::new();
        log(&mut ring, 100);
        // Every 7th frame lost, acks included.
        let (text, _) = download(&mut ring, |n, _| n % 7 == 0);
        assert_eq!(text, expected(0..100));
        // Every ack lost twice before it gets through.
        let mut ring = LogRing::<4096>::new();
        log(&mut ring, 100);
        let mut acks = 0;
        let (text, _) = download(&mut ring, |_, frame| {
            if matches!(frame, LogFrame::Ack { .. }) {
                acks += 1;
                acks % 3 != 0
            } else {
                false
            }
        });
        assert_eq!(text, expected(0..100));
    }

    #[test]
    fn a_damaged_block_is_not_taken() {
        let mut ring = LogRing::<512>::new();
        log(&mut ring, 3);
        let mut sender = LogSender::new();
        assert_eq!(sender.request(&mut ring, 0), Action::SendBlock);
        let mut frames: Vec<LogFrame> = sender
            .block_frames()
            .map(|p| LogFrame::from_bytes(p.as_slice()).unwrap())
            .collect();
        if let LogFrame::Data { bytes, .. } = &mut frames[0] {
            bytes[0] ^= 1;
        }
        let mut receiver = LogReceiver::new();
        let verdicts: Vec<bool> = frames
            .into_iter()
            .map(|f| receiver.frame(f) == Received::Damaged)
            .collect();
        assert_eq!(verdicts.last(), Some(&true));
        // Unanswered, the node sends it again, then gives up and keeps it.
        let mut now = 0;
        for _ in 0..MAX_RETRIES {
            now += ACK_TIMEOUT_MS;
            assert_eq!(sender.poll(now), Action::SendBlock);
        }
        assert_eq!(sender.poll(now + ACK_TIMEOUT_MS), Action::Idle);
        assert!(ring.waiting() > 0);
    }

    #[test]
    fn an_empty_log_ends_at_once_and_reports_lost_bytes() {
        let mut ring = LogRing::<64>::new();
        log(&mut ring, 10);
        let mut sender = LogSender::new();
        // Read what is there, then nothing is left but the count.
        let (_, lost) = download(&mut ring, |_, _| false);
        assert!(lost > 0);
        assert_eq!(
            sender.request(&mut ring, 0),
            Action::SendEnd(LogFrame::End { seq: 0, lost: 0 })
        );
    }
}
