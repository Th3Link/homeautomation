//! Pushing firmware to a CAN node, the bus master's side.
//!
//! **Blocks** ([`gateway_core::ota`]): the image goes over in blocks of
//! [`BLOCK_SIZE`] bytes, each announced by a header with its CRC-16, sent
//! back to back and answered by the node: acked when it is safe, nak'ed (and
//! then sent again) when frames are missing or damaged. There are no fixed
//! delays; the pace is the node's answers. The logic of one block is in
//! [`gateway_core::ota_sender::BlockSender`], which is tested against the
//! node's side with a bus that loses frames.
//!
//! **Legacy** (`FlashStart`/`FlashWrite`/…, [`gateway_core::can_ota`]): the
//! original unacknowledged stream with its fixed delays. Used for the old
//! STM32 nodes, for updating all devices of a type at once (a broadcast can't
//! be acknowledged by one sender), and as a fallback for a node that doesn't
//! know the block protocol yet (its firmware is older than this one): a node
//! that does not answer `Begin` is sent the old way.
//!
//! One update at a time ([`start_device`]/[`start_type`], then [`data`]
//! with the image in pieces of any size, then [`complete`]).

use crate::config::{self, config};
use crate::console_log;
use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer};
use gateway_core::can_message_type::CanMessageType;
use gateway_core::can_ota::{self, FlashWriter};
use gateway_core::device_type::DeviceType;
use gateway_core::ota::{
    self, OtaControl, OtaErrorCode, BLOCK_SIZE, BLOCK_TIMEOUT_MS, END_TIMEOUT_MS,
};
use gateway_core::ota_sender::{BlockSender, Failure, Verdict};
use heapless::Vec;

/// A node that does not answer `Begin` within this time (in two tries) is sent
/// the image the old way.
const FALLBACK_AFTER_MS: u32 = 1500;
const BEGIN_TRIES: u32 = 2;
/// How often `End` is sent again if no answer comes.
const END_TRIES: u32 = 3;

/// What the node answered, with the device key it came from. Fed by
/// [`crate::translate`].
static REPLIES: Channel<CriticalSectionRawMutex, (u32, OtaControl), 4> = Channel::new();

/// An `OtaControl` frame came in from `device_key`.
pub fn reply(device_key: u32, control: OtaControl) {
    let _ = REPLIES.try_send((device_key, control));
}

/// How an update is going, for the web API.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Progress {
    /// Image bytes the node has acknowledged (blocks) or that were sent
    /// (legacy).
    pub bytes: u32,
    /// Blocks sent again.
    pub retries: u32,
    pub protocol: Option<Protocol>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Blocks,
    Legacy,
}

impl Protocol {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Blocks => "blocks",
            Self::Legacy => "legacy",
        }
    }
}

/// Why an update failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateError {
    /// The node did not answer.
    NoAnswer,
    /// A block could not be got through.
    TooManyRetries,
    /// The sender and the node lost track of each other.
    OutOfOrder,
    /// The node ended the update.
    Node(OtaErrorCode),
    /// `data`/`complete` without a running update.
    NotStarted,
}

impl UpdateError {
    pub const fn message(self) -> &'static str {
        match self {
            Self::NoAnswer => "the device did not answer",
            Self::TooManyRetries => "the bus lost too many frames, a block could not be sent",
            Self::OutOfOrder => "the device and the gateway disagree about the block order",
            Self::Node(OtaErrorCode::BadBegin) => "the device refused the update",
            Self::Node(OtaErrorCode::TooLarge) => "the image does not fit the device",
            Self::Node(OtaErrorCode::NotStarted) => "the device lost the update",
            Self::Node(OtaErrorCode::VerifyFailed) => "the device could not verify the image",
            Self::Node(OtaErrorCode::FlashFailed) => "the device could not write its flash",
            Self::Node(OtaErrorCode::Incomplete) => "the device is missing blocks",
            Self::NotStarted => "no update is running",
        }
    }
}

impl From<Failure> for UpdateError {
    fn from(failure: Failure) -> Self {
        match failure {
            Failure::TooManyRetries => Self::TooManyRetries,
            Failure::Node(code) => Self::Node(code),
            Failure::OutOfOrder => Self::OutOfOrder,
        }
    }
}

struct BlockSession {
    key: u32,
    size: u32,
    /// Index of the block being collected.
    index: u16,
    /// The block being collected.
    buffer: Vec<u8, BLOCK_SIZE>,
    /// Bytes in blocks the node has acked.
    acked: u32,
    retries: u32,
}

struct LegacySession {
    target_base: u32,
    writer: FlashWriter,
    sent: u32,
}

// One update at a time, kept in one static: the difference in size is no waste.
#[allow(clippy::large_enum_variant)]
enum Session {
    Blocks(BlockSession),
    Legacy(LegacySession),
}

static SESSION: Mutex<CriticalSectionRawMutex, Option<Session>> = Mutex::new(None);

/// How far the running update is.
pub async fn progress() -> Progress {
    match SESSION.lock().await.as_ref() {
        Some(Session::Blocks(s)) => Progress {
            bytes: s.acked,
            retries: s.retries,
            protocol: Some(Protocol::Blocks),
        },
        Some(Session::Legacy(s)) => Progress {
            bytes: s.sent,
            retries: 0,
            protocol: Some(Protocol::Legacy),
        },
        None => Progress::default(),
    }
}

fn control_id(key: u32) -> u32 {
    key + u32::from(u8::from(CanMessageType::OtaControl))
}

fn data_id(key: u32) -> u32 {
    key + u32::from(u8::from(CanMessageType::OtaData))
}

async fn send_control(key: u32, control: OtaControl) {
    if let Ok(payload) = control.to_payload() {
        crate::can::send_raw(control_id(key), payload.as_slice(), false).await;
    }
}

/// The next answer from `key`, or `None` after `timeout_ms`.
async fn wait_reply(key: u32, timeout_ms: u32) -> Option<OtaControl> {
    let wait = async {
        loop {
            let (from, control) = REPLIES.receive().await;
            if from == key {
                return control;
            }
        }
    };
    match select(
        wait,
        Timer::after(Duration::from_millis(u64::from(timeout_ms))),
    )
    .await
    {
        Either::First(control) => Some(control),
        Either::Second(()) => None,
    }
}

/// Starts an update of one device. Tries the block protocol; a device that
/// doesn't know it (or is a legacy one) is sent the old way.
pub async fn start_device(key: u32, size: u32, crc: u32) -> Result<Protocol, UpdateError> {
    let device_type = DeviceType::from((key >> 16) as u8);
    let legacy_only = matches!(
        device_type,
        DeviceType::LegacyRelais | DeviceType::LegacyLamps
    );
    if !legacy_only {
        // Answers of an earlier update are not for this one.
        while REPLIES.try_receive().is_ok() {}
        for _ in 0..BEGIN_TRIES {
            send_control(
                key,
                OtaControl::Begin {
                    block_size: BLOCK_SIZE as u16,
                    size,
                    crc32: crc,
                },
            )
            .await;
            match wait_reply(key, FALLBACK_AFTER_MS).await {
                Some(OtaControl::Ack { next: 0 }) => {
                    *SESSION.lock().await = Some(Session::Blocks(BlockSession {
                        key,
                        size,
                        index: 0,
                        buffer: Vec::new(),
                        acked: 0,
                        retries: 0,
                    }));
                    return Ok(Protocol::Blocks);
                }
                Some(OtaControl::Error(code)) => return Err(UpdateError::Node(code)),
                _ => {}
            }
        }
        console_log!("can_update: {key:#x} does not answer Begin, updating the old way");
    }
    start_legacy(key, size, crc).await;
    Ok(Protocol::Legacy)
}

/// Starts an update of every device of a type at once, the old way (a
/// broadcast cannot be acknowledged by one sender).
pub async fn start_type(device_type: u8, size: u32, crc: u32) {
    start_legacy(can_ota::base_key_for_type(device_type), size, crc).await;
}

async fn start_legacy(target_base: u32, size: u32, crc: u32) {
    let update_delay_ms = config()
        .await
        .get_u32(config::Key::UpdateDelay)
        .await
        .unwrap_or(can_ota::DEFAULT_UPDATE_DELAY_MS);

    for step in can_ota::start_steps(size, crc) {
        Timer::after(step.wait).await;
        send_step(target_base, &step).await;
    }
    Timer::after(can_ota::start_trailing_delay()).await;

    *SESSION.lock().await = Some(Session::Legacy(LegacySession {
        target_base,
        writer: FlashWriter::new(size, update_delay_ms),
        sent: 0,
    }));
}

/// Sends the next piece of the image. With blocks this returns when the
/// blocks that were completed are acknowledged: the caller is paced by the
/// bus.
pub async fn data(mut chunk: &[u8]) -> Result<(), UpdateError> {
    let mut guard = SESSION.lock().await;
    match guard.as_mut().ok_or(UpdateError::NotStarted)? {
        Session::Legacy(session) => {
            let target_base = session.target_base;
            session.sent += chunk.len() as u32;
            while let Some(step) = session.writer.next_chunk(&mut chunk) {
                Timer::after(step.wait).await;
                crate::can::send_raw(
                    can_ota::target_id(target_base, CanMessageType::FlashWrite),
                    &step.bytes,
                    false,
                )
                .await;
                if let Some(extra) = step.extra_wait_after {
                    Timer::after(extra).await;
                }
            }
            Ok(())
        }
        Session::Blocks(session) => {
            while !chunk.is_empty() {
                let room = block_len(session) - session.buffer.len();
                let take = room.min(chunk.len());
                let _ = session.buffer.extend_from_slice(&chunk[..take]);
                chunk = &chunk[take..];
                if session.buffer.len() == block_len(session) {
                    send_block(session).await?;
                }
            }
            Ok(())
        }
    }
}

/// How long the block being collected is: a full one, or what is left of the
/// image.
fn block_len(session: &BlockSession) -> usize {
    let left = session.size - session.acked;
    (left as usize).min(BLOCK_SIZE)
}

/// Sends the collected block until the node has it.
async fn send_block(session: &mut BlockSession) -> Result<(), UpdateError> {
    let key = session.key;
    let mut sender = BlockSender::new(session.index, &session.buffer);
    'send: loop {
        send_control(key, sender.header()).await;
        for frame in ota::frames(&session.buffer) {
            crate::can::send_raw(data_id(key), &frame, false).await;
        }
        loop {
            match sender.answer(wait_reply(key, BLOCK_TIMEOUT_MS).await) {
                Verdict::Acked => break 'send,
                Verdict::Retry => {
                    session.retries += 1;
                    continue 'send;
                }
                Verdict::Wait => continue,
                Verdict::Failed(failure) => {
                    send_control(key, OtaControl::Abort).await;
                    return Err(failure.into());
                }
            }
        }
    }
    session.acked += session.buffer.len() as u32;
    session.index += 1;
    session.buffer.clear();
    Ok(())
}

/// Everything has been sent: finishes the update. The node verifies the image
/// and restarts into it.
pub async fn complete() -> Result<(), UpdateError> {
    let session = SESSION.lock().await.take().ok_or(UpdateError::NotStarted)?;
    match session {
        Session::Legacy(session) => {
            for step in can_ota::complete_steps() {
                Timer::after(step.wait).await;
                send_step(session.target_base, &step).await;
            }
            Ok(())
        }
        Session::Blocks(mut session) => {
            if !session.buffer.is_empty() {
                send_block(&mut session).await?;
            }
            let key = session.key;
            for _ in 0..END_TRIES {
                send_control(key, OtaControl::End).await;
                match wait_reply(key, END_TIMEOUT_MS).await {
                    Some(OtaControl::Done) => return Ok(()),
                    Some(OtaControl::Error(code)) => return Err(UpdateError::Node(code)),
                    _ => {}
                }
            }
            Err(UpdateError::NoAnswer)
        }
    }
}

/// Gives the update up. With blocks the node is told to forget it (the old
/// firmware keeps running, nothing was activated); the old protocol's way to
/// stop is to finish.
pub async fn abort() {
    let session = SESSION.lock().await.take();
    match session {
        Some(Session::Legacy(session)) => {
            for step in can_ota::complete_steps() {
                Timer::after(step.wait).await;
                send_step(session.target_base, &step).await;
            }
        }
        Some(Session::Blocks(session)) => send_control(session.key, OtaControl::Abort).await,
        None => {}
    }
}

async fn send_step(target_base: u32, step: &can_ota::CanOtaStep) {
    let id = can_ota::target_id(target_base, step.msg_type);
    crate::can::send_raw(id, &step.data, step.request).await;
}
