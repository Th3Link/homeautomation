//! The log lines on MQTT, and the nodes' logs over CAN (ADR 0026).
//!
//! - **Nodes:** every [`interval_s`] the gateway downloads the logs of the
//!   nodes in the device list, one after the other (`LogDownload`,
//!   [`gateway_core::log_download`]), and publishes their lines on
//!   `canbus/device_log/<device>`. A node takes a line out of its log once
//!   the gateway acknowledged it, so every line comes once. A node that
//!   doesn't answer (an old one without the protocol) is left out for a
//!   day. The web UI can download one node's log at once ([`request`]);
//!   its lines are kept for the page ([`with_web`]) and go to MQTT too.
//! - **The gateway:** every [`GATEWAY_EVERY`] the lines logged since the
//!   last time go to `canbus/gateway/log`.
//!
//! Both only while the primary broker is connected and [`to_mqtt`] is on
//! (it is unless switched off). The web UI's download works without MQTT.

use crate::config::{config, Key};
use crate::console_log;
use crate::device_list::DEVICE_LIST;
use alloc::string::String;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use embassy_futures::select::{select3, Either3};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::mutex::Mutex;
use embassy_sync::signal::Signal;
use embassy_time::{with_timeout, Duration, Instant, Timer};
use gateway_core::can_id::CanId;
use gateway_core::can_message_type::CanMessageType;
use gateway_core::can_ota::target_id;
use gateway_core::log_download::{LogFrame, LogReceiver, Received, BLOCK_FRAMES};
use gateway_core::topics::device::{device_log_topic, GATEWAY_LOG_TOPIC};

/// The default of [`interval_s`]: five minutes.
pub const DEFAULT_INTERVAL_S: u32 = 300;
/// How often the gateway's own new lines go out.
const GATEWAY_EVERY: Duration = Duration::from_secs(10);
/// The first round of node downloads, after boot: once the nodes are known.
const FIRST_ROUND: Duration = Duration::from_secs(60);
/// A node that hasn't answered the request by then doesn't know it.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(1);
/// Silence in the middle of a download: the node gave a block up (after
/// its retries); asked again, it sends it once more.
const STALL_TIMEOUT: Duration = Duration::from_secs(4);
const STALL_RETRIES: u32 = 3;
/// How long the nodes that didn't answer are left out.
const SKIP_FOR: Duration = Duration::from_secs(24 * 3600);
/// The most text kept for the web UI.
const WEB_CAP: usize = 8 * 1024;
/// An MQTT message is a line, cut to this.
const LINE_CAP: usize = 200;

static TO_MQTT: AtomicBool = AtomicBool::new(true);
static INTERVAL_S: AtomicU32 = AtomicU32::new(DEFAULT_INTERVAL_S);
static SETTINGS_CHANGED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// The `LogDownload` frames from the nodes, with their device keys (a
/// block's frames come back to back). Fed by [`crate::translate`].
static FRAMES: Channel<CriticalSectionRawMutex, (u32, LogFrame), { BLOCK_FRAMES + 8 }> =
    Channel::new();

/// A `LogDownload` frame came in from `device_key`.
pub fn frame(device_key: u32, frame: LogFrame) {
    let _ = FRAMES.try_send((device_key, frame));
}

/// Whether the log lines go to MQTT.
pub fn to_mqtt() -> bool {
    TO_MQTT.load(Ordering::Relaxed)
}

/// Seconds between two rounds of node downloads; 0: none.
pub fn interval_s() -> u32 {
    INTERVAL_S.load(Ordering::Relaxed)
}

/// Reads the settings. Once, at boot.
pub async fn init() {
    let mut store = config().await;
    if let Some(on) = store.get_bool(Key::LogsToMqtt).await {
        TO_MQTT.store(on, Ordering::Relaxed);
    }
    if let Some(seconds) = store.get_u32(Key::NodeLogInterval).await {
        INTERVAL_S.store(seconds, Ordering::Relaxed);
    }
}

/// Switches the log lines on MQTT on or off, and stores it.
pub async fn set_to_mqtt(on: bool) -> Result<(), ()> {
    config().await.set_bool(Key::LogsToMqtt, on).await?;
    TO_MQTT.store(on, Ordering::Relaxed);
    SETTINGS_CHANGED.signal(());
    Ok(())
}

/// Sets the time between two rounds of node downloads (0: none), and
/// stores it.
pub async fn set_interval_s(seconds: u32) -> Result<(), ()> {
    config()
        .await
        .set_u32(Key::NodeLogInterval, seconds)
        .await?;
    INTERVAL_S.store(seconds, Ordering::Relaxed);
    SETTINGS_CHANGED.signal(());
    Ok(())
}

/// How the web UI's download went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Running,
    Done,
    /// The node didn't answer: it doesn't know the protocol, or is gone.
    NoAnswer,
    /// It answered, but stopped in the middle.
    Failed,
}

impl State {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Done => "done",
            Self::NoAnswer => "no_answer",
            Self::Failed => "failed",
        }
    }
}

/// The web UI's download: one node's lines, kept until the next one.
pub struct WebLog {
    pub device_key: u32,
    pub state: State,
    pub text: String,
    /// Bytes the node overwrote before they were downloaded.
    pub lost: u32,
}

static WEB: Mutex<CriticalSectionRawMutex, Option<WebLog>> = Mutex::new(None);
static WEB_REQUEST: Signal<CriticalSectionRawMutex, u32> = Signal::new();

/// Starts downloading `device_key`'s log for the web UI; `false` while a
/// download for it runs.
pub async fn request(device_key: u32) -> bool {
    let mut web = WEB.lock().await;
    if web.as_ref().is_some_and(|w| w.state == State::Running) {
        return false;
    }
    *web = Some(WebLog {
        device_key,
        state: State::Running,
        text: String::new(),
        lost: 0,
    });
    WEB_REQUEST.signal(device_key);
    true
}

/// Runs `f` on the web UI's download (`None` before the first).
pub async fn with_web<R>(f: impl FnOnce(Option<&WebLog>) -> R) -> R {
    f(WEB.lock().await.as_ref())
}

enum Outcome {
    Done { lost: u16 },
    NoAnswer,
    Failed,
}

async fn custom_string(device_key: u32) -> heapless::String<10> {
    let mut name = heapless::String::new();
    let _ = name.push_str(DEVICE_LIST.lock().await.entry(device_key));
    name
}

/// Publishes `text`'s lines on `topic`, one a message.
async fn publish_lines(topic: &str, text: &str) {
    for line in text.lines().filter(|line| !line.is_empty()) {
        let mut end = line.len().min(LINE_CAP);
        while !line.is_char_boundary(end) {
            end -= 1;
        }
        crate::mqtt::publish_wait(topic, &line[..end]).await;
    }
}

/// Downloads `device_key`'s log; each block's text goes to the web UI
/// (`web`) and to MQTT (if on and connected).
async fn download(device_key: u32, web: bool) -> Outcome {
    let can_id = target_id(device_key, CanMessageType::LogDownload);
    let topic = device_log_topic(device_key, &custom_string(device_key).await);
    while FRAMES.try_receive().is_ok() {}
    crate::can::send_raw(can_id, &[], true).await;

    let mut receiver = LogReceiver::new();
    let mut answered = false;
    let mut stalls = 0;
    loop {
        let timeout = if answered {
            STALL_TIMEOUT
        } else {
            ANSWER_TIMEOUT
        };
        let Ok((key, frame)) = with_timeout(timeout, FRAMES.receive()).await else {
            if !answered {
                return Outcome::NoAnswer;
            }
            stalls += 1;
            if stalls > STALL_RETRIES {
                return Outcome::Failed;
            }
            crate::can::send_raw(can_id, &[], true).await;
            continue;
        };
        if key != device_key {
            continue;
        }
        answered = true;
        match receiver.frame(frame) {
            Received::Block { seq, data, .. } => {
                stalls = 0;
                // Whole lines; a character the node had to cut stays whole
                // as far as it can.
                let text = match core::str::from_utf8(data) {
                    Ok(text) => text,
                    Err(e) => core::str::from_utf8(&data[..e.valid_up_to()]).unwrap_or(""),
                };
                if web {
                    if let Some(w) = WEB.lock().await.as_mut() {
                        let room = WEB_CAP.saturating_sub(w.text.len());
                        let mut end = text.len().min(room);
                        while !text.is_char_boundary(end) {
                            end -= 1;
                        }
                        w.text.push_str(&text[..end]);
                    }
                }
                let ack = LogFrame::Ack { seq }.to_payload();
                crate::can::send_raw(can_id, ack.as_slice(), false).await;
                if to_mqtt() && crate::mqtt::is_connected(0) {
                    publish_lines(&topic, text).await;
                }
            }
            Received::Again { seq } => {
                let ack = LogFrame::Ack { seq }.to_payload();
                crate::can::send_raw(can_id, ack.as_slice(), false).await;
            }
            Received::End { lost } => return Outcome::Done { lost },
            Received::Nothing | Received::Damaged => {}
        }
    }
}

/// The web UI's download of `device_key`.
async fn web_download(device_key: u32) {
    let outcome = download(device_key, true).await;
    if let Some(w) = WEB.lock().await.as_mut() {
        (w.state, w.lost) = match outcome {
            Outcome::Done { lost } => (State::Done, u32::from(lost)),
            Outcome::NoAnswer => (State::NoAnswer, 0),
            Outcome::Failed => (State::Failed, 0),
        };
    }
}

/// The gateway's own device key: it isn't asked.
fn own_key() -> u32 {
    CanId::new(
        crate::can::DEVICE_TYPE.load(Ordering::Relaxed),
        crate::can::DEVICE_ID.load(Ordering::Relaxed),
        CanMessageType::Available,
    )
    .device_key()
}

/// One round: every node in the device list that knows the protocol.
async fn round(skipped: &mut heapless::Vec<u32, 32>) {
    let keys: heapless::Vec<u32, { crate::device_list::DEVICE_LIST_SIZE }> =
        DEVICE_LIST.lock().await.iter().map(|e| e.id).collect();
    let own = own_key();
    for key in keys {
        if key == own || skipped.contains(&key) {
            continue;
        }
        // The web UI's request goes first.
        if let Some(web) = WEB_REQUEST.try_take() {
            web_download(web).await;
        }
        if !(to_mqtt() && crate::mqtt::is_connected(0)) {
            return;
        }
        match download(key, false).await {
            Outcome::Done { lost: 0 } => {}
            Outcome::Done { lost } => {
                let topic = device_log_topic(key, &custom_string(key).await);
                let mut line: heapless::String<64> = heapless::String::new();
                let _ = core::fmt::write(
                    &mut line,
                    format_args!("({lost} bytes of log overwritten before the download)"),
                );
                crate::mqtt::publish_wait(&topic, &line).await;
            }
            Outcome::NoAnswer => {
                let _ = skipped.push(key);
            }
            Outcome::Failed => console_log!("node log: download from {key:#x} failed"),
        }
    }
}

#[embassy_executor::task]
pub async fn node_log_task() {
    // On the heap: the receiver and the device keys would otherwise be
    // static and take the main stack's room (see `crate::heap`).
    crate::heap::boxed(run).await
}

async fn run() {
    let mut skipped: heapless::Vec<u32, 32> = heapless::Vec::new();
    let mut skipped_since = Instant::now();
    let mut next_round = Instant::now() + FIRST_ROUND;
    loop {
        let interval = interval_s();
        let at = if interval == 0 {
            Instant::MAX
        } else {
            next_round
        };
        match select3(WEB_REQUEST.wait(), Timer::at(at), SETTINGS_CHANGED.wait()).await {
            Either3::First(key) => web_download(key).await,
            Either3::Second(()) => {
                if skipped_since.elapsed() > SKIP_FOR {
                    skipped.clear();
                    skipped_since = Instant::now();
                }
                if to_mqtt() && crate::mqtt::is_connected(0) {
                    round(&mut skipped).await;
                }
                next_round = Instant::now() + Duration::from_secs(u64::from(interval_s()));
            }
            Either3::Third(()) => {
                next_round = Instant::now() + Duration::from_secs(u64::from(interval_s()));
            }
        }
    }
}

/// The gateway's own lines to MQTT, every [`GATEWAY_EVERY`]: those logged
/// since the last time (from boot on: the oldest the log still has).
#[embassy_executor::task]
pub async fn gateway_log_task() {
    // On the heap: the text read would be static otherwise.
    crate::heap::boxed(gateway_log).await
}

async fn gateway_log() {
    let mut from = 0u32;
    loop {
        Timer::after(GATEWAY_EVERY).await;
        if !(to_mqtt() && crate::mqtt::is_connected(0)) {
            continue;
        }
        loop {
            let (_, text, next) = crate::log_buffer::read::<1024>(from);
            from = next.max(from);
            if text.is_empty() {
                break;
            }
            publish_lines(GATEWAY_LOG_TOPIC, &text).await;
        }
    }
}
