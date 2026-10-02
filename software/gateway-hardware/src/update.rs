//! Self-firmware OTA update, streamed in from the `/update/data` HTTP
//! upload (`Update.cpp`/`IUpdate.hpp` in the original, which wraps
//! `esp_ota_*` directly). Reuses `esp-hal-ota` the same way
//! `cancomponent-rs/cc-hardware`'s `update.rs` does for its CAN-triggered
//! OTA, just fed from HTTP bytes instead of CAN frames.

use crate::console_log;
use crate::flash::SharedFlash;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use esp_hal_ota::Ota;

static OTA: Mutex<CriticalSectionRawMutex, Option<Ota<SharedFlash>>> = Mutex::new(None);

/// Marks the currently-running app partition valid (confirming a prior OTA
/// update didn't need a rollback). Call once at boot.
pub async fn init() {
    match Ota::new(SharedFlash) {
        Ok(mut ota) => {
            ota.ota_mark_app_valid().ok();
        }
        Err(e) => {
            console_log!("update: ota init failed: {e:?}");
        }
    }
}

/// Begins a self-update: opens the inactive OTA partition for a `size`-byte
/// image with the given CRC32 (`Update::start` / `IUpdate::start` in the
/// original). Returns `false` if the OTA partition couldn't be opened.
pub async fn start(size: u32, crc: u32) -> bool {
    match Ota::new(SharedFlash) {
        Ok(mut ota) => {
            if ota.ota_begin(size, crc).is_ok() {
                *OTA.lock().await = Some(ota);
                true
            } else {
                console_log!("update: ota_begin failed");
                *OTA.lock().await = None;
                false
            }
        }
        Err(e) => {
            console_log!("update: ota init failed: {e:?}");
            *OTA.lock().await = None;
            false
        }
    }
}

/// Writes one chunk of the uploaded firmware image
/// (`Update::data`/`IUpdate::data`).
pub async fn write(chunk: &[u8]) -> bool {
    let mut guard = OTA.lock().await;
    match guard.as_mut() {
        Some(ota) => match ota.ota_write_chunk(chunk) {
            Ok(_) => true,
            Err(e) => {
                console_log!("update: write failed: {e:?}");
                false
            }
        },
        None => false,
    }
}

/// Finalizes the update: verifies the image and, on success, reboots into
/// it (`Update::complete`/`IUpdate::complete`). Never returns on success.
pub async fn complete() -> bool {
    let mut guard = OTA.lock().await;
    let Some(ota) = guard.as_mut() else {
        return false;
    };
    match ota.ota_flush(true, true) {
        Ok(()) => {
            drop(guard);
            esp_hal::system::software_reset();
        }
        Err(e) => {
            console_log!("update: verify failed: {e:?}");
            false
        }
    }
}

/// Aborts an in-progress update (`Update::abort`/`IUpdate::abort`).
pub async fn abort() {
    *OTA.lock().await = None;
}
