//! Asking a heating controller for its state: an R frame on
//! [`CanMessageType::HeatingState`], answered by a data frame of the same
//! type ([`gateway_core::heating_message::HeatingState`]). The controller
//! never sends it by itself.
//!
//! [`translate`](crate::translate) feeds the answers in, [`query`] is for the
//! web server.

use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use gateway_core::can_message_type::CanMessageType;
use gateway_core::heating_message::HeatingState;

/// How long a controller has to answer.
const ANSWER_WITHIN: Duration = Duration::from_millis(400);

/// The last state frame that arrived, and from which device.
static LATEST: Mutex<CriticalSectionRawMutex, Option<(u32, HeatingState)>> = Mutex::new(None);
static ARRIVED: Signal<CriticalSectionRawMutex, ()> = Signal::new();
/// One question at a time: there is one signal for the answers.
static ONE_AT_A_TIME: Mutex<CriticalSectionRawMutex, ()> = Mutex::new(());

/// A state frame came in from `device_key`.
pub async fn observe(device_key: u32, state: HeatingState) {
    *LATEST.lock().await = Some((device_key, state));
    ARRIVED.signal(());
}

/// Asks `device_key` for its state. `None` if it does not answer in time (an
/// older firmware, or not a heating controller).
pub async fn query(device_key: u32) -> Option<HeatingState> {
    let _guard = ONE_AT_A_TIME.lock().await;
    *LATEST.lock().await = None;
    ARRIVED.reset();
    let id = device_key + u32::from(u8::from(CanMessageType::HeatingState));
    crate::can::send_raw(id, &[], true).await;

    let answer = async {
        loop {
            ARRIVED.wait().await;
            if let Some((key, state)) = *LATEST.lock().await {
                if key == device_key {
                    return state;
                }
            }
        }
    };
    match select(answer, Timer::after(ANSWER_WITHIN)).await {
        Either::First(state) => Some(state),
        Either::Second(()) => None,
    }
}
