//! The node's log over CAN (`LogDownload`, ADR 0026): every
//! [`console_log!`](crate::console_log) line also goes into a
//! [`LogRing`] of [`SIZE`] bytes, and the gateway downloads it from there.
//! A line leaves the ring once the gateway acknowledged the block it came
//! in, so it is downloaded exactly once; when the ring is full the oldest
//! lines go, and the next download says how many bytes that were. The
//! protocol is [`cancomponents_core::log_download`].

use crate::can::send_can_message;
use cancomponents_core::can_message_type::CanMessageType;
use cancomponents_core::log_download::{Action, LogFrame, LogRing, LogSender};
use core::cell::RefCell;
use critical_section::Mutex;
use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Instant, Timer};

/// Bytes of log kept.
pub const SIZE: usize = 4 * 1024;

static RING: Mutex<RefCell<LogRing<SIZE>>> = Mutex::new(RefCell::new(LogRing::new()));

enum Event {
    Request,
    Ack(u8),
}

static EVENTS: Channel<CriticalSectionRawMutex, Event, 4> = Channel::new();

/// Starts keeping the log lines. Once, early in `main`, before the first
/// line worth keeping.
pub fn install() {
    common_hardware::logging::set_sink(record);
}

fn record(line: core::fmt::Arguments<'_>) {
    let uptime_ms = Instant::now().as_millis();
    critical_section::with(|cs| RING.borrow_ref_mut(cs).push_line(uptime_ms, line));
}

/// Starts the task that answers the downloads.
pub fn init(spawner: &Spawner) {
    spawner.spawn(log_task().unwrap());
}

/// A `LogDownload` frame from the gateway: a request (R frame) or an ack.
pub fn handler(data: &[u8], request: bool) {
    let event = if request {
        Event::Request
    } else {
        match LogFrame::from_bytes(data) {
            Ok(LogFrame::Ack { seq }) => Event::Ack(seq),
            _ => return,
        }
    };
    let _ = EVENTS.try_send(event);
}

fn now_ms() -> u64 {
    Instant::now().as_millis()
}

#[embassy_executor::task]
async fn log_task() {
    let mut sender = LogSender::new();
    loop {
        let deadline = sender
            .deadline_ms()
            .map_or(Instant::MAX, Instant::from_millis);
        let action = match select(EVENTS.receive(), Timer::at(deadline)).await {
            Either::First(Event::Request) => {
                critical_section::with(|cs| sender.request(&mut RING.borrow_ref_mut(cs), now_ms()))
            }
            Either::First(Event::Ack(seq)) => {
                critical_section::with(|cs| sender.ack(seq, &mut RING.borrow_ref_mut(cs), now_ms()))
            }
            Either::Second(()) => sender.poll(now_ms()),
        };
        match action {
            Action::SendBlock => {
                for frame in sender.block_frames() {
                    send_can_message(CanMessageType::LogDownload, frame.as_slice(), false).await;
                }
            }
            Action::SendEnd(end) => {
                send_can_message(
                    CanMessageType::LogDownload,
                    end.to_payload().as_slice(),
                    false,
                )
                .await;
            }
            Action::Idle => {}
        }
    }
}
