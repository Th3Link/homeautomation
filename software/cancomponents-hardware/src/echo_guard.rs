//! CAN bus keepalive: periodically sends an `Echo` RTR frame and restarts
//! the device if too many intervals pass without any `Echo` traffic being
//! observed on the bus (see `can::dispatch`'s `CanMessageType::Echo` arm,
//! which calls [`dispatch_echo`] on every `Echo` frame seen). Muted while
//! the bus is [`crate::can::SILENCE`]d (see [`disable_echo`]/`can::silence`).
//!
//! The restart is how a node recovers from a CAN bus that has gone bad or
//! from having lost its gateway. It can be switched off with the console's
//! `echo-restart` command (persisted), e.g. for a device on the bench that
//! has no gateway to answer, or one that must not reset on its own.

use crate::can::send_can_message;
use crate::config::{self, config};
use crate::console_log;
use cancomponents_core::can_message_type::CanMessageType;
use core::sync::atomic::{AtomicBool, Ordering};
use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, Timer};

const MAX_ECHO_MISS: usize = 3;
const ECHO_INTERVALL: Duration = Duration::from_secs(30);

/// `true` = echo traffic observed (resets the miss counter and re-arms the
/// guard); `false` = disable the guard entirely (see [`disable_echo`]).
pub static ECHO_CHANNEL: Channel<CriticalSectionRawMutex, bool, 2> = Channel::new();

/// Whether missing echoes restart the device (the default).
static RESTART_ENABLED: AtomicBool = AtomicBool::new(true);

pub async fn init(spawner: &Spawner) {
    // Unset means on; only an explicit 0 turns the restart off.
    if let Some(setting) = config().await.get_u8(config::Key::EchoRestart).await {
        RESTART_ENABLED.store(setting != 0, Ordering::Relaxed);
    }
    spawner.spawn(echo_guard_task().unwrap());
}

/// Whether the device restarts when the echoes stay away.
pub fn restart_enabled() -> bool {
    RESTART_ENABLED.load(Ordering::Relaxed)
}

/// Switches the restart on or off and persists the choice; takes effect at
/// once. `Err` if it couldn't be persisted (then nothing changes).
pub async fn set_restart_enabled(enabled: bool) -> Result<(), ()> {
    config()
        .await
        .set_u8(config::Key::EchoRestart, u8::from(enabled))
        .await?;
    RESTART_ENABLED.store(enabled, Ordering::Relaxed);
    Ok(())
}

/// Signals that `Echo` traffic was observed, resetting the miss counter.
pub async fn dispatch_echo() {
    ECHO_CHANNEL.send(true).await;
}

/// Suspends the keepalive (and its restart-on-miss behavior) until
/// [`dispatch_echo`] re-enables it.
pub async fn disable_echo() {
    ECHO_CHANNEL.send(false).await;
}

#[embassy_executor::task]
pub async fn echo_guard_task() {
    console_log!("echo_guard_task started");
    let mut miss_count = 0;
    let mut enabled = true;
    loop {
        if enabled {
            let delay = Timer::after(ECHO_INTERVALL);
            let echo_recv = ECHO_CHANNEL.receive();
            match select(echo_recv, delay).await {
                Either::First(enable) => {
                    enabled = enable;
                    miss_count = 0;
                    Timer::after(ECHO_INTERVALL).await;
                }
                Either::Second(_) => {
                    miss_count += 1;
                    if miss_count > MAX_ECHO_MISS {
                        if restart_enabled() {
                            console_log!("Echos missed, restarting");
                            Timer::after(Duration::from_secs(2)).await;
                            esp_hal::system::software_reset();
                        }
                        // Restart switched off: just keep probing, without
                        // letting the counter grow forever.
                        miss_count = MAX_ECHO_MISS;
                    }
                }
            }
            let data: [u8; 0] = [];
            send_can_message(CanMessageType::Echo, &data, true).await;
        } else {
            enabled = ECHO_CHANNEL.receive().await;
        }
    }
}
