//! `GET /api/v1/system`.

use super::error::ApiError;
use super::json::json;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use picoserve::response::{IntoResponse, StatusCode};
use serde::Serialize;

static RESTART: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// Restarts the gateway a moment after [`restart`] answered, so that the
/// answer still gets out.
#[embassy_executor::task]
pub async fn restart_task() {
    RESTART.wait().await;
    Timer::after(Duration::from_millis(500)).await;
    esp_hal::system::software_reset();
}

/// Restarts the gateway shortly.
pub fn request_restart() {
    RESTART.signal(());
}

pub async fn restart() -> impl IntoResponse {
    request_restart();
    StatusCode::ACCEPTED
}

#[derive(Serialize)]
pub struct System {
    firmware_version: &'static str,
    uptime_s: u64,
    heap_used: usize,
    heap_free: usize,
    /// Main stack bytes never used since boot: the margin left.
    stack_untouched: usize,
    stack_size: usize,
}

pub async fn get_system() -> Result<impl IntoResponse, ApiError> {
    json(&System {
        firmware_version: env!("VERGEN_GIT_DESCRIBE"),
        uptime_s: embassy_time::Instant::now().as_secs(),
        heap_used: esp_alloc::HEAP.used(),
        heap_free: esp_alloc::HEAP.free(),
        stack_untouched: crate::stack::untouched(),
        stack_size: crate::stack::size(),
    })
    .await
}
