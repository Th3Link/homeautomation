//! Drives [`gateway_core::can_ota`]'s pure step sequences with real CAN
//! sends and delays — the hardware counterpart to `CANUpdate.cpp`. Only
//! one CAN-OTA session can be in flight at a time (matches the original's
//! single `CANUpdate` instance).

use crate::config::{self, config};
use crate::console_log;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::Timer;
use gateway_core::can_ota::{self, FlashWriter};

struct Session {
    target_base: u32,
    writer: FlashWriter,
}

static SESSION: Mutex<CriticalSectionRawMutex, Option<Session>> = Mutex::new(None);

/// `CANUpdate::by_type_start` — targets every device of `device_type`.
pub async fn by_type_start(device_type: u8, size: u32, crc: u32) {
    start(can_ota::base_key_for_type(device_type), size, crc).await;
}

/// `CANUpdate::by_uid_start` — targets a single already-resolved device.
pub async fn by_uid_start(target_key: u32, size: u32, crc: u32) {
    start(target_key, size, crc).await;
}

async fn start(target_base: u32, size: u32, crc: u32) {
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

    *SESSION.lock().await = Some(Session {
        target_base,
        writer: FlashWriter::new(size, update_delay_ms),
    });
}

/// `CANUpdate::data` — streams `chunk` as `FlashWrite` frames.
pub async fn data(mut chunk: &[u8]) {
    let mut guard = SESSION.lock().await;
    let Some(session) = guard.as_mut() else {
        console_log!("can_update: data() with no session in progress");
        return;
    };
    let target_base = session.target_base;

    while let Some(step) = session.writer.next_chunk(&mut chunk) {
        Timer::after(step.wait).await;
        crate::can::send_raw(
            can_ota::target_id(
                target_base,
                gateway_core::can_message_type::CanMessageType::FlashWrite,
            ),
            &step.bytes,
            false,
        )
        .await;
        if let Some(extra) = step.extra_wait_after {
            Timer::after(extra).await;
        }
    }
}

/// `CANUpdate::complete` — finishes the transfer and restarts the target
/// into application mode.
pub async fn complete() {
    let Some(session) = SESSION.lock().await.take() else {
        return;
    };
    for step in can_ota::complete_steps() {
        Timer::after(step.wait).await;
        send_step(session.target_base, &step).await;
    }
}

/// `CANUpdate::abort` — the original's `abort()` just calls `complete()`
/// verbatim, so this does too.
pub async fn abort() {
    complete().await;
}

async fn send_step(target_base: u32, step: &can_ota::CanOtaStep) {
    let id = can_ota::target_id(target_base, step.msg_type);
    crate::can::send_raw(id, &step.data, step.request).await;
}
