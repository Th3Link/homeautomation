//! The time of day, from SNTP (RFC 4330). The gateway has no RTC, so until
//! a server has answered the time is unknown; certificate checks then fall
//! back to "not before the firmware was built" ([`clock`]).
//!
//! MbedTLS takes its time from here too (the `hook-wall-clock` hooks): it
//! refuses certificate dates while the time is unknown, which only the EST
//! client notices (the web server checks no certificates).

use crate::console_log;
use core::sync::atomic::{AtomicU32, Ordering};
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::{IpAddress, Stack};
use embassy_time::{Duration, Instant, Timer};
use gateway_core::x509::check::Clock;
use gateway_core::x509::time::{parse_rfc3339, DateTime};

/// Unix seconds at boot (`Instant` zero); 0 while unknown.
static UNIX_AT_BOOT: AtomicU32 = AtomicU32::new(0);

/// Seconds from 1900 (NTP's epoch) to 1970.
const NTP_TO_UNIX: u64 = 2_208_988_800;
const NTP_PORT: u16 = 123;
const RESYNC: Duration = Duration::from_secs(6 * 3600);
/// Until the first answer: the first request after the link comes up is often
/// lost while the neighbour is still being resolved.
const RETRY: Duration = Duration::from_secs(10);

/// The current Unix time, if a server has answered since boot.
pub fn now() -> Option<i64> {
    let base = UNIX_AT_BOOT.load(Ordering::Relaxed);
    (base != 0).then(|| i64::from(base) + Instant::now().as_secs() as i64)
}

/// When the firmware was built (a lower bound for the time).
pub fn build_time() -> i64 {
    parse_rfc3339(env!("VERGEN_BUILD_TIMESTAMP")).unwrap_or(0)
}

/// What is known about the time, for the certificate rules.
pub fn clock() -> Clock {
    match now() {
        Some(now) => Clock::Known(now),
        None => Clock::AtLeast(build_time()),
    }
}

/// Asks the configured NTP server for the time, now and every six hours
/// (every 10 s until a server has answered).
#[embassy_executor::task]
pub async fn sntp_task(stack: Stack<'static>) {
    let mut rx_meta = [PacketMetadata::EMPTY; 2];
    let mut tx_meta = [PacketMetadata::EMPTY; 2];
    let mut rx = [0u8; 128];
    let mut tx = [0u8; 128];
    loop {
        stack.wait_config_up().await;
        let server = crate::cert::ntp_server().await;
        let mut socket = UdpSocket::new(stack, &mut rx_meta, &mut rx, &mut tx_meta, &mut tx);
        let synced = match socket.bind(0) {
            // On the heap, like the EST client: the DNS lookup's future is
            // large and only needed for a moment.
            Ok(()) => crate::heap::boxed(|| query(stack, &mut socket, &server)).await,
            Err(_) => false,
        };
        drop(socket);
        Timer::after(if synced { RESYNC } else { RETRY }).await;
    }
}

/// Asks the addresses of `server` in turn (IPv6 first, see
/// [`crate::resolve`]) until one answers.
async fn query(stack: Stack<'static>, socket: &mut UdpSocket<'_>, server: &str) -> bool {
    let addresses = crate::resolve::resolve(stack, server).await;
    if addresses.is_empty() {
        console_log!("sntp: cannot resolve {server}");
        return false;
    }
    for address in addresses {
        if ask(socket, address, server).await {
            return true;
        }
    }
    false
}

async fn ask(socket: &mut UdpSocket<'_>, address: IpAddress, server: &str) -> bool {
    // Version 4, mode 3 (client); everything else zero.
    let mut request = [0u8; 48];
    request[0] = 0x23;
    if socket.send_to(&request, (address, NTP_PORT)).await.is_err() {
        return false;
    }
    let mut answer = [0u8; 48];
    let received =
        embassy_time::with_timeout(Duration::from_secs(5), socket.recv_from(&mut answer)).await;
    let Ok(Ok((48.., _))) = received else {
        return false;
    };
    // Mode 4 (server), a stratum, and a transmit timestamp.
    let stratum = answer[1];
    if answer[0] & 0x07 != 4 || stratum == 0 || stratum > 15 {
        return false;
    }
    let seconds = u64::from(u32::from_be_bytes([
        answer[40], answer[41], answer[42], answer[43],
    ]));
    let Some(unix) = seconds.checked_sub(NTP_TO_UNIX) else {
        return false;
    };
    let unix = unix as i64;
    if unix < build_time() {
        console_log!("sntp: {server} answered a time before the firmware was built");
        return false;
    }
    let first = now().is_none();
    let base = unix - Instant::now().as_secs() as i64;
    UNIX_AT_BOOT.store(base as u32, Ordering::Relaxed);
    if first {
        let t = DateTime::from_unix(unix);
        console_log!(
            "sntp: time is {:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
            t.year,
            t.month,
            t.day,
            t.hour,
            t.minute,
            t.second
        );
        crate::cert::time_known();
    }
    true
}

/// MbedTLS' time sources.
struct Clocks;

impl mbedtls_rs::sys::hook::wall_clock::MbedtlsWallClock for Clocks {
    fn instant(&self) -> Option<mbedtls_rs::sys::tm> {
        let t = DateTime::from_unix(now()?);
        let days = gateway_core::x509::time::days_from_civil(t.year, t.month, t.day);
        let year_start = gateway_core::x509::time::days_from_civil(t.year, 1, 1);
        // SAFETY: `tm` is a plain C struct; all-zero is a valid value.
        let mut tm: mbedtls_rs::sys::tm = unsafe { core::mem::zeroed() };
        tm.tm_sec = t.second as _;
        tm.tm_min = t.minute as _;
        tm.tm_hour = t.hour as _;
        tm.tm_mday = t.day as _;
        tm.tm_mon = (t.month - 1) as _;
        tm.tm_year = (t.year - 1900) as _;
        // 1970-01-01 was a Thursday.
        tm.tm_wday = (days + 4).rem_euclid(7) as _;
        tm.tm_yday = (days - year_start) as _;
        Some(tm)
    }
}

impl mbedtls_rs::sys::hook::timer::MbedtlsTimer for Clocks {
    fn now(&self) -> mbedtls_rs::sys::mbedtls_ms_time_t {
        Instant::now().as_millis() as _
    }
}

static CLOCKS: Clocks = Clocks;

/// Gives MbedTLS the clocks; before any TLS session.
pub fn hook_mbedtls() {
    // SAFETY: called once at boot before any MbedTLS use; `CLOCKS` is a
    // 'static stateless value.
    unsafe {
        mbedtls_rs::sys::hook::wall_clock::hook_wall_clock(Some(&CLOCKS));
        mbedtls_rs::sys::hook::timer::hook_timer(Some(&CLOCKS));
    }
}
