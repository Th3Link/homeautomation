//! Firmware update over CAN in blocks: the frame formats and the checksum.
//! The two ends of it are [`crate::ota_sender`] (the bus master that pushes
//! the image) and [`crate::ota_receiver`] (the node that takes it).
//!
//! The image goes over as **blocks** of [`BLOCK_SIZE`] bytes. A block is
//! announced by a header frame (index, length, CRC-16) and followed by its
//! bytes in [`CanMessageType::OtaData`] frames of 8 bytes each, sent back to
//! back — no pause between frames. The receiver collects the block, checks
//! its length and CRC and answers: an *ack* when the block is safe, a *nak*
//! when it is not (frames missing, wrong CRC), and the sender sends the
//! block again. Flash is only written where the sender is waiting for an
//! answer, so the receiver never has to take frames while it is busy.
//!
//! Control frames ([`CanMessageType::OtaControl`]) carry an operation in the
//! low 4 bits of byte 0:
//!
//! | op | frame | direction |
//! |---|---|---|
//! | 0 | [`OtaControl::Begin`] | to the node |
//! | 1 | [`OtaControl::Block`] | to the node |
//! | 2 | [`OtaControl::End`] | to the node |
//! | 3 | [`OtaControl::Abort`] | to the node |
//! | 4 | [`OtaControl::Ack`] | from the node |
//! | 5 | [`OtaControl::Nak`] | from the node |
//! | 6 | [`OtaControl::Done`] | from the node |
//! | 7 | [`OtaControl::Error`] | from the node |
//!
//! [`CanMessageType::OtaControl`]: crate::can_message_type::CanMessageType::OtaControl
//! [`CanMessageType::OtaData`]: crate::can_message_type::CanMessageType::OtaData

use crate::{DecodeError, EncodeError, Payload};

/// Bytes in a block (the last block of an image may be shorter). A multiple
/// of 256 that divides [`SECTOR_SIZE`]; the receiver learns it from
/// [`OtaControl::Begin`], so both ends only need to agree on what they
/// support.
pub const BLOCK_SIZE: usize = 1024;
/// The flash sector the receiver writes at a time (it collects blocks until a
/// sector is full).
pub const SECTOR_SIZE: usize = 4096;
/// Data bytes in one `OtaData` frame.
pub const FRAME_BYTES: usize = 8;
/// The image size is sent in 3 bytes.
pub const MAX_IMAGE_SIZE: u32 = 0x00FF_FFFF;

/// How long the sender waits for the answer to `Begin` (the node may have to
/// erase), in milliseconds.
pub const BEGIN_TIMEOUT_MS: u32 = 10_000;
/// How long the sender waits for the answer to a block, after its last frame.
pub const BLOCK_TIMEOUT_MS: u32 = 1_000;
/// How long the sender waits for `Done` after `End` (the node verifies the
/// whole image).
pub const END_TIMEOUT_MS: u32 = 15_000;
/// How long a receiver waits for the rest of a block before it asks for it
/// again.
pub const STALL_MS: u32 = 300;
/// How often the sender repeats a block that was refused or not answered
/// before it gives up.
pub const MAX_RETRIES: u8 = 8;

const _: () = {
    assert!(BLOCK_SIZE >= 256 && BLOCK_SIZE <= SECTOR_SIZE);
    assert!(BLOCK_SIZE.is_multiple_of(256));
    assert!(SECTOR_SIZE.is_multiple_of(BLOCK_SIZE));
};

/// Whether `block_size` is one a receiver takes: a multiple of 256 that
/// divides a sector.
pub const fn supported_block_size(block_size: usize) -> bool {
    block_size >= 256
        && block_size <= SECTOR_SIZE
        && block_size.is_multiple_of(256)
        && SECTOR_SIZE.is_multiple_of(block_size)
}

/// CRC-16/CCITT-FALSE (polynomial 0x1021, initial value 0xFFFF) — the checksum
/// of a block. `crc16(b"123456789") == 0x29B1`.
pub fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        crc ^= u16::from(byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// How many `OtaData` frames a block of `len` bytes takes.
pub const fn data_frames(len: usize) -> usize {
    len.div_ceil(FRAME_BYTES)
}

/// The data frames of a block: 8 bytes each, the last one zero-padded.
pub fn frames(block: &[u8]) -> impl Iterator<Item = [u8; FRAME_BYTES]> + '_ {
    block.chunks(FRAME_BYTES).map(|chunk| {
        let mut frame = [0u8; FRAME_BYTES];
        frame[..chunk.len()].copy_from_slice(chunk);
        frame
    })
}

/// Why a node refused a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NakReason {
    /// Frames of the block are missing.
    Short = 1,
    /// All frames came, but the CRC does not match.
    BadCrc = 2,
    /// The flash write failed.
    Flash = 3,
    /// The header does not fit the update (length).
    BadHeader = 4,
    /// A block other than the next one expected was sent; the index in the
    /// frame is the one expected.
    OutOfOrder = 5,
}

impl NakReason {
    pub const fn from_byte(byte: u8) -> Option<Self> {
        Some(match byte {
            1 => Self::Short,
            2 => Self::BadCrc,
            3 => Self::Flash,
            4 => Self::BadHeader,
            5 => Self::OutOfOrder,
            _ => return None,
        })
    }
}

/// Why a node ended the update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtaErrorCode {
    /// `Begin` is not acceptable (block size, size 0).
    BadBegin = 1,
    /// The image does not fit the update partition.
    TooLarge = 2,
    /// A block or `End` came without a running update.
    NotStarted = 3,
    /// The image did not verify (size, CRC-32).
    VerifyFailed = 4,
    /// The flash could not be written.
    FlashFailed = 5,
    /// `End` came before every block did.
    Incomplete = 6,
}

impl OtaErrorCode {
    pub const fn from_byte(byte: u8) -> Option<Self> {
        Some(match byte {
            1 => Self::BadBegin,
            2 => Self::TooLarge,
            3 => Self::NotStarted,
            4 => Self::VerifyFailed,
            5 => Self::FlashFailed,
            6 => Self::Incomplete,
            _ => return None,
        })
    }
}

const OP_BEGIN: u8 = 0;
const OP_BLOCK: u8 = 1;
const OP_END: u8 = 2;
const OP_ABORT: u8 = 3;
const OP_ACK: u8 = 4;
const OP_NAK: u8 = 5;
const OP_DONE: u8 = 6;
const OP_ERROR: u8 = 7;

/// One `OtaControl` frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtaControl {
    /// Start an update: the blocks will be `block_size` bytes, the image
    /// `size` bytes with this CRC-32.
    Begin {
        block_size: u16,
        size: u32,
        crc32: u32,
    },
    /// The next block is block `index`, `len` bytes with this CRC-16; its
    /// data frames follow.
    Block { index: u16, len: u16, crc16: u16 },
    /// Every block has been sent: verify the image and switch to it.
    End,
    /// Give the update up.
    Abort,
    /// `Begin` was accepted (`next` = 0) or block `next - 1` and all before
    /// it are safe: `next` is the block expected next.
    Ack { next: u16 },
    /// Block `index` was refused.
    Nak { index: u16, reason: NakReason },
    /// The image verified; the node restarts into it.
    Done,
    /// The update failed for good.
    Error(OtaErrorCode),
}

impl OtaControl {
    pub fn to_payload(&self) -> Result<Payload, EncodeError> {
        let mut b = [0u8; 8];
        let len = match *self {
            Self::Begin {
                block_size,
                size,
                crc32,
            } => {
                let block_size = usize::from(block_size);
                if !supported_block_size(block_size) || size > MAX_IMAGE_SIZE {
                    return Err(EncodeError);
                }
                b[0] = OP_BEGIN | (((block_size / 256 - 1) as u8) << 4);
                b[1..4].copy_from_slice(&size.to_le_bytes()[..3]);
                b[4..8].copy_from_slice(&crc32.to_le_bytes());
                8
            }
            Self::Block { index, len, crc16 } => {
                b[0] = OP_BLOCK;
                b[1..3].copy_from_slice(&index.to_le_bytes());
                b[3..5].copy_from_slice(&len.to_le_bytes());
                b[5..7].copy_from_slice(&crc16.to_le_bytes());
                7
            }
            Self::End => {
                b[0] = OP_END;
                1
            }
            Self::Abort => {
                b[0] = OP_ABORT;
                1
            }
            Self::Ack { next } => {
                b[0] = OP_ACK;
                b[1..3].copy_from_slice(&next.to_le_bytes());
                3
            }
            Self::Nak { index, reason } => {
                b[0] = OP_NAK;
                b[1..3].copy_from_slice(&index.to_le_bytes());
                b[3] = reason as u8;
                4
            }
            Self::Done => {
                b[0] = OP_DONE;
                1
            }
            Self::Error(code) => {
                b[0] = OP_ERROR;
                b[1] = code as u8;
                2
            }
        };
        Ok(Payload::new(b, len))
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, DecodeError> {
        let first = *data.first().ok_or(DecodeError)?;
        let need = |n: usize| {
            if data.len() >= n {
                Ok(())
            } else {
                Err(DecodeError)
            }
        };
        let u16_at = |i: usize| u16::from_le_bytes([data[i], data[i + 1]]);
        Ok(match first & 0x0F {
            OP_BEGIN => {
                need(8)?;
                Self::Begin {
                    block_size: ((first >> 4) as u16 + 1) * 256,
                    size: u32::from_le_bytes([data[1], data[2], data[3], 0]),
                    crc32: u32::from_le_bytes([data[4], data[5], data[6], data[7]]),
                }
            }
            OP_BLOCK => {
                need(7)?;
                Self::Block {
                    index: u16_at(1),
                    len: u16_at(3),
                    crc16: u16_at(5),
                }
            }
            OP_END => Self::End,
            OP_ABORT => Self::Abort,
            OP_ACK => {
                need(3)?;
                Self::Ack { next: u16_at(1) }
            }
            OP_NAK => {
                need(4)?;
                Self::Nak {
                    index: u16_at(1),
                    reason: NakReason::from_byte(data[3]).ok_or(DecodeError)?,
                }
            }
            OP_DONE => Self::Done,
            OP_ERROR => {
                need(2)?;
                Self::Error(OtaErrorCode::from_byte(data[1]).ok_or(DecodeError)?)
            }
            _ => return Err(DecodeError),
        })
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    fn bytes(message: OtaControl) -> std::vec::Vec<u8> {
        message.to_payload().unwrap().as_slice().to_vec()
    }

    #[test]
    fn crc16_check_value() {
        assert_eq!(crc16(b"123456789"), 0x29B1);
        assert_eq!(crc16(&[]), 0xFFFF);
        assert_ne!(crc16(&[0, 0, 0]), crc16(&[0, 0]));
    }

    #[test]
    fn frames_pad_the_last_one() {
        let block: std::vec::Vec<u8> = (0..20).collect();
        let all: std::vec::Vec<[u8; 8]> = frames(&block).collect();
        assert_eq!(all.len(), data_frames(20));
        assert_eq!(all[0], [0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(all[2], [16, 17, 18, 19, 0, 0, 0, 0]);
        assert_eq!(data_frames(BLOCK_SIZE), BLOCK_SIZE / 8);
    }

    #[test]
    fn begin_layout() {
        let begin = OtaControl::Begin {
            block_size: 1024,
            size: 0x0006_5432,
            crc32: 0xDEAD_BEEF,
        };
        // block size 1024 -> 3 in the high nibble.
        assert_eq!(
            bytes(begin),
            [0x30, 0x32, 0x54, 0x06, 0xEF, 0xBE, 0xAD, 0xDE]
        );
        assert_eq!(OtaControl::from_bytes(&bytes(begin)), Ok(begin));
        for block_size in [256, 512, 1024, 2048, 4096] {
            let message = OtaControl::Begin {
                block_size,
                size: 1,
                crc32: 0,
            };
            assert_eq!(OtaControl::from_bytes(&bytes(message)), Ok(message));
        }
    }

    #[test]
    fn begin_refuses_what_it_cannot_hold() {
        let ok = OtaControl::Begin {
            block_size: 1024,
            size: MAX_IMAGE_SIZE,
            crc32: 0,
        };
        assert!(ok.to_payload().is_ok());
        for (block_size, size) in [
            (1024, MAX_IMAGE_SIZE + 1),
            (1000, 10),
            (128, 10),
            (8192, 10),
            (3072, 10),
        ] {
            let bad = OtaControl::Begin {
                block_size,
                size,
                crc32: 0,
            };
            assert_eq!(bad.to_payload(), Err(EncodeError), "{block_size} {size}");
        }
    }

    #[test]
    fn block_and_replies_layout() {
        let block = OtaControl::Block {
            index: 0x0102,
            len: 1024,
            crc16: 0xABCD,
        };
        assert_eq!(bytes(block), [1, 0x02, 0x01, 0x00, 0x04, 0xCD, 0xAB]);
        assert_eq!(OtaControl::from_bytes(&bytes(block)), Ok(block));

        let ack = OtaControl::Ack { next: 7 };
        assert_eq!(bytes(ack), [4, 7, 0]);
        assert_eq!(OtaControl::from_bytes(&bytes(ack)), Ok(ack));

        let nak = OtaControl::Nak {
            index: 9,
            reason: NakReason::BadCrc,
        };
        assert_eq!(bytes(nak), [5, 9, 0, 2]);
        assert_eq!(OtaControl::from_bytes(&bytes(nak)), Ok(nak));

        assert_eq!(bytes(OtaControl::End), [2]);
        assert_eq!(bytes(OtaControl::Abort), [3]);
        assert_eq!(bytes(OtaControl::Done), [6]);
        let error = OtaControl::Error(OtaErrorCode::VerifyFailed);
        assert_eq!(bytes(error), [7, 4]);
        assert_eq!(OtaControl::from_bytes(&bytes(error)), Ok(error));
    }

    #[test]
    fn bad_frames_are_rejected() {
        assert_eq!(OtaControl::from_bytes(&[]), Err(DecodeError));
        assert_eq!(OtaControl::from_bytes(&[0x30, 1, 2]), Err(DecodeError));
        assert_eq!(OtaControl::from_bytes(&[1, 0, 0]), Err(DecodeError));
        assert_eq!(OtaControl::from_bytes(&[4, 1]), Err(DecodeError));
        assert_eq!(OtaControl::from_bytes(&[5, 0, 0, 99]), Err(DecodeError));
        assert_eq!(OtaControl::from_bytes(&[7, 99]), Err(DecodeError));
        for op in 8..16 {
            assert_eq!(OtaControl::from_bytes(&[op]), Err(DecodeError));
        }
        // The argument nibble of the other ops is ignored.
        assert_eq!(OtaControl::from_bytes(&[0xF2]), Ok(OtaControl::End));
    }
}
