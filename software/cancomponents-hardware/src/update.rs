//! OTA firmware update over CAN, in blocks (`OtaControl`/`OtaData`, see
//! [`cancomponents_core::ota`]): the image comes in blocks that are CRC-checked
//! and acknowledged one by one, a block that went wrong is sent again, flash is
//! only written while the sender waits for the answer. The state machine is
//! [`cancomponents_core::ota_receiver::OtaReceiver`]; this module carries out
//! what it asks for.
//!
//! The node does **not** speak the original streaming protocol
//! (`FlashStart`/`FlashWrite`/…) any more: the gateway still can (for the old
//! STM32 nodes and for a node whose firmware is older than this), the nodes
//! only know the block protocol.
//!
//! A new image is only *pending* after the flash: it has to prove itself, see
//! [`confirm`].
use crate::can::{send_can_message, SILENCE};
use crate::console_log;
use crate::echo_guard::{disable_echo, ECHO_CHANNEL};
use crate::error::{report_error, Component, ErrorCode, Severity};
use crate::flash::SharedFlash;
use cancomponents_core::can_message_type::CanMessageType;
use cancomponents_core::ota::OtaControl;
use cancomponents_core::ota_receiver::{Effect, OtaReceiver};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Instant, Timer};
use esp_hal_ota::Ota;
use num_enum::FromPrimitive;

static OTA: Mutex<CriticalSectionRawMutex, Option<Ota<SharedFlash>>> = Mutex::new(None);

/// `local_code` values reported alongside `Component::Ota`/`Component::Update`
/// `DeviceError`s, distinguishing where in the OTA flow something failed.
#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, FromPrimitive)]
pub enum UpdateErrorCode {
    #[num_enum(default)]
    Unknown = 0,
    InvalidData = 1,
    Begin = 2,
    Init = 3,
    Write = 4,
    NotStarted = 5,
    VerifyFailed = 6,
}

/// How long a node whose echo guard may not restart it waits before it
/// confirms the running image anyway (see [`confirm`]).
const CONFIRM_GRACE: Duration = Duration::from_secs(120);

static CONFIRMED: AtomicBool = AtomicBool::new(false);

/// Starts the tasks of the block update.
///
/// A freshly flashed image is *pending*: if the node resets before the image
/// confirmed itself (see [`confirm`]) the bootloader goes back to the old one.
pub async fn init(spawner: &Spawner) {
    if Ota::new(SharedFlash).is_err() {
        report_error(
            Component::Ota,
            ErrorCode::Unknown,
            Severity::RecoverableError,
            UpdateErrorCode::Init as u8,
            &[0u8, 0u8, 0u8],
        )
        .await;
    }
    spawner.spawn(ota_tick_task().unwrap());
    spawner.spawn(confirm_task().unwrap());
}

/// Marks the running image as good, once: from then on the bootloader keeps
/// it. Done when the node has been *seen working on the bus* — the first
/// `Echo` it gets answered ([`crate::echo_guard`]) — and not merely at the
/// start of the boot, so that an image that starts but cannot talk is not
/// kept. A node whose echo guard may not restart it (a device on the bench
/// without a gateway) confirms after [`CONFIRM_GRACE`] instead; one whose
/// guard may restart it and hears nothing resets itself and the old image
/// comes back.
pub async fn confirm() {
    if CONFIRMED.swap(true, Ordering::Relaxed) {
        return;
    }
    match Ota::new(SharedFlash) {
        Ok(mut ota) => {
            ota.ota_mark_app_valid().ok();
            console_log!("update: the running image is confirmed");
        }
        Err(_) => {
            // Try again at the next echo.
            CONFIRMED.store(false, Ordering::Relaxed);
        }
    }
}

#[embassy_executor::task]
async fn confirm_task() {
    Timer::after(CONFIRM_GRACE).await;
    if !crate::echo_guard::restart_enabled() {
        confirm().await;
    }
}

// ---- the block update ------------------------------------------------------

static RECEIVER: Mutex<CriticalSectionRawMutex, OtaReceiver> = Mutex::new(OtaReceiver::new());

/// How often a block whose frames have stopped is looked at.
const TICK: Duration = Duration::from_millis(50);
/// The node restarts this long after it said `Done`, so that the frame gets out.
const RESTART_DELAY: Duration = Duration::from_millis(1000);

fn now_ms() -> u64 {
    Instant::now().as_millis()
}

/// During an update the node keeps quiet — no sensor readings, no `Echo`
/// probes (their missing answers must not restart the node half way) — and
/// talks again afterwards.
async fn quiet(on: bool) {
    if on {
        *SILENCE.lock().await = true;
        disable_echo().await;
    } else {
        *SILENCE.lock().await = false;
        ECHO_CHANNEL.send(true).await;
    }
}

/// When the last update frame came in, in ms since boot (wrapping, which is
/// fine for comparing against a short time).
static LAST_FRAME_MS: AtomicU32 = AtomicU32::new(0);
/// An update whose sender has been silent this long is given up.
const IDLE_ABORT_MS: u32 = 30_000;

/// Handles an `OtaControl` frame.
pub async fn ota_control(data: &[u8], remote_request: bool) {
    if remote_request {
        return;
    }
    let Ok(frame) = OtaControl::from_bytes(data) else {
        report_error(
            Component::Update,
            ErrorCode::InvalidData,
            Severity::Warning,
            UpdateErrorCode::InvalidData as u8,
            &[CanMessageType::OtaControl as u8, data.len() as u8, 0u8],
        )
        .await;
        return;
    };
    let mut receiver = RECEIVER.lock().await;
    let now = now_ms();
    LAST_FRAME_MS.store(now as u32, Ordering::Relaxed);
    carry_out(&mut receiver, |r| r.control(frame, now)).await;
    if frame == OtaControl::Abort {
        *OTA.lock().await = None;
        quiet(false).await;
    }
}

/// Handles an `OtaData` frame: 8 bytes of the block being received.
pub async fn ota_data(data: &[u8], remote_request: bool) {
    if remote_request {
        return;
    }
    let mut receiver = RECEIVER.lock().await;
    let now = now_ms();
    LAST_FRAME_MS.store(now as u32, Ordering::Relaxed);
    carry_out(&mut receiver, |r| r.data(data, now)).await;
}

/// A block whose frames stopped coming is asked for again.
#[embassy_executor::task]
async fn ota_tick_task() {
    loop {
        Timer::after(TICK).await;
        let mut receiver = RECEIVER.lock().await;
        if receiver.active() {
            let now = now_ms();
            carry_out(&mut receiver, |r| r.tick(now)).await;
            // A sender that has gone away leaves nothing behind.
            if (now as u32).wrapping_sub(LAST_FRAME_MS.load(Ordering::Relaxed)) > IDLE_ABORT_MS {
                console_log!("update: nobody sends any more, giving up");
                carry_out(&mut receiver, |r| r.control(OtaControl::Abort, now)).await;
                *OTA.lock().await = None;
                quiet(false).await;
            }
        }
    }
}

/// Does what the receiver asks for until it has nothing more to say.
async fn carry_out(
    receiver: &mut OtaReceiver,
    first: impl for<'a> FnOnce(&'a mut OtaReceiver) -> Effect<'a>,
) {
    let mut effect = first(receiver);
    loop {
        effect = match effect {
            Effect::None => return,
            Effect::Reply(reply) => {
                reply_with(reply).await;
                return;
            }
            Effect::Begin { size, crc32 } => {
                let ok = begin_flash(size, crc32).await;
                receiver.begun(ok)
            }
            Effect::WriteSector { offset, data } => {
                let ok = write_sector(offset, data).await;
                receiver.sector_written(ok)
            }
            Effect::Verify => {
                let ok = finish_flash().await;
                receiver.verified(ok)
            }
        };
    }
}

/// Sends `reply` and takes care of what it means for the rest of the node.
async fn reply_with(reply: OtaControl) {
    match reply {
        // The update is on: the node keeps quiet but for the update.
        OtaControl::Ack { next: 0 } => quiet(true).await,
        OtaControl::Error(code) => {
            console_log!("update: failed ({code:?})");
            *OTA.lock().await = None;
            quiet(false).await;
        }
        _ => {}
    }
    if let Ok(payload) = reply.to_payload() {
        send_can_message(CanMessageType::OtaControl, payload.as_slice(), false).await;
    }
    if reply == OtaControl::Done {
        console_log!("update: done, restarting");
        Timer::after(RESTART_DELAY).await;
        esp_hal::system::software_reset();
    }
}

async fn begin_flash(size: u32, crc: u32) -> bool {
    console_log!("update: begin, {size} bytes, crc {crc:#x}");
    match Ota::new(SharedFlash) {
        Ok(mut ota) => {
            if ota.ota_begin(size, crc).is_ok() {
                *OTA.lock().await = Some(ota);
                true
            } else {
                *OTA.lock().await = None;
                false
            }
        }
        Err(_) => {
            *OTA.lock().await = None;
            false
        }
    }
}

/// Writes the next sector of the image. The blocks arrive in order, so this
/// is always the place right after the last one.
async fn write_sector(_offset: u32, data: &[u8]) -> bool {
    match OTA.lock().await.as_mut() {
        Some(ota) => match ota.ota_write_chunk(data) {
            Ok(_) => true,
            Err(e) => {
                console_log!("update: write failed: {e:?}");
                false
            }
        },
        None => false,
    }
}

/// Verifies the whole image (size, CRC-32, read back from flash) and makes it
/// the one to boot — as a *pending* image, see [`confirm`].
async fn finish_flash() -> bool {
    match OTA.lock().await.as_mut() {
        Some(ota) => match ota.ota_flush(true, true) {
            Ok(()) => true,
            Err(e) => {
                console_log!("update: verify failed: {e:?}");
                false
            }
        },
        None => false,
    }
}
