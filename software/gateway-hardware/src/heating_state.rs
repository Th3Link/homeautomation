//! The heating controllers' manual mode as the bus reports it, see
//! [`gateway_core::heating_manual`]. Fed by [`crate::translate`], read by the
//! web server and by the MQTT command dispatch.

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use gateway_core::heating_manual::{ManualStatus, ManualTable};

/// More heating controllers than this on one bus are not expected.
const CONTROLLERS: usize = 4;

static MANUAL: Mutex<CriticalSectionRawMutex, ManualTable<CONTROLLERS>> =
    Mutex::new(ManualTable::new());

pub async fn observe(device_key: u32, state: gateway_core::heating_message::HeatingManualState) {
    MANUAL
        .lock()
        .await
        .observe(device_key, state, crate::sensor_list::now_seconds());
}

/// Whether `device_key`'s controller is in manual mode, and with which
/// relays on.
pub async fn manual_status(device_key: u32) -> ManualStatus {
    MANUAL
        .lock()
        .await
        .status(device_key, crate::sensor_list::now_seconds())
}
