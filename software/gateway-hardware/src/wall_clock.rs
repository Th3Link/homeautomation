//! The time of day, from SNTP (RFC 4330) or, for a server set as
//! `nts://host[:port]`, from NTS ([`crate::nts`], RFC 8915), which
//! authenticates it. The gateway has no RTC, so until a server has answered
//! the time is unknown; certificate checks then fall back to "not before the
//! firmware was built" ([`clock`]).
//!
//! MbedTLS takes its time from here too (the `hook-wall-clock` hooks): it
//! refuses certificate dates while the time is unknown, which only the EST
//! client notices (the web server checks no certificates).

use crate::console_log;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::{IpAddress, Stack};
use embassy_time::{Duration, Instant, Timer};
use gateway_core::nts::{parse_server, Server};
use gateway_core::x509::check::Clock;
use gateway_core::x509::time::{parse_rfc3339, DateTime};

/// Unix seconds at boot (`Instant` zero); 0 while unknown.
static UNIX_AT_BOOT: AtomicU32 = AtomicU32::new(0);
/// Whether the time was last set by NTS.
static AUTHENTICATED: AtomicBool = AtomicBool::new(false);
/// A time MbedTLS is given while the time is unknown, see [`assuming`];
/// 0 for none.
static ASSUMED: AtomicU32 = AtomicU32::new(0);

/// Seconds from 1900 (NTP's epoch) to 1970.
const NTP_TO_UNIX: u64 = 2_208_988_800;
const NTP_PORT: u16 = 123;
const RESYNC: Duration = Duration::from_secs(6 * 3600);
/// Until the first answer: the first request after the link comes up is often
/// lost while the neighbour is still being resolved.
const RETRY: Duration = Duration::from_secs(10);
/// After a failed NTS key exchange (a TLS handshake each time).
const KE_RETRY: Duration = Duration::from_secs(60);

/// The current Unix time, if a server has answered since boot.
pub fn now() -> Option<i64> {
    let base = UNIX_AT_BOOT.load(Ordering::Relaxed);
    (base != 0).then(|| i64::from(base) + Instant::now().as_secs() as i64)
}

/// Whether the current time came from NTS (authenticated).
pub fn authenticated() -> bool {
    now().is_some() && AUTHENTICATED.load(Ordering::Relaxed)
}

/// When `instant` was, in Unix seconds, once the time is known.
pub fn unix_at(instant: Instant) -> Option<i64> {
    Some(now()? - Instant::now().saturating_duration_since(instant).as_secs() as i64)
}

/// Runs `f` (MbedTLS, without awaiting) with MbedTLS taking `unix` as the
/// time if the real one isn't known: it doesn't check certificate chains
/// without a time at all.
pub fn assuming<R>(unix: i64, f: impl FnOnce() -> R) -> R {
    ASSUMED.store(unix as u32, Ordering::Relaxed);
    let result = f();
    ASSUMED.store(0, Ordering::Relaxed);
    result
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

/// An NTS request with a cookie and seven placeholders is about 956 bytes.
pub(crate) const DATAGRAM_CAP: usize = 1024;

/// The one time client uses either plain SNTP or authenticated NTP. Both
/// borrow these socket rings from upper SRAM1 for an exchange.
pub(crate) struct DatagramBuffers {
    pub rx: [u8; DATAGRAM_CAP],
    pub tx: [u8; DATAGRAM_CAP],
}

impl DatagramBuffers {
    fn take() -> &'static mut Self {
        use core::mem::MaybeUninit;
        use core::sync::atomic::{AtomicBool, Ordering};
        static TAKEN: AtomicBool = AtomicBool::new(false);
        #[unsafe(link_section = ".dram2_uninit")]
        static mut BUFFERS: MaybeUninit<DatagramBuffers> = MaybeUninit::uninit();
        assert!(!TAKEN.swap(true, Ordering::Relaxed));
        // SAFETY: the single time task owns both arrays for the rest of the boot.
        unsafe {
            let ptr = (&raw mut BUFFERS).cast::<Self>();
            ptr.write_bytes(0, 1);
            &mut *ptr
        }
    }
}

/// Asks the configured time server for the time, now and every six hours
/// (until a server has answered: every 10 s, or every minute after a
/// failed NTS key exchange).
#[embassy_executor::task]
pub async fn sntp_task(stack: Stack<'static>) {
    let buffers = DatagramBuffers::take();
    loop {
        crate::ethernet::wait_ready().await;
        let server = crate::cert::ntp_server().await;
        let wait = match parse_server(&server) {
            Some(Server::Nts { host, port }) => {
                match crate::nts::synchronise(stack, host, port, buffers).await {
                    crate::nts::Outcome::Synchronised => RESYNC,
                    crate::nts::Outcome::Failed => RETRY,
                    crate::nts::Outcome::KeyExchangeFailed => KE_RETRY,
                }
            }
            Some(Server::Plain(host)) => {
                if query(stack, host, buffers).await {
                    RESYNC
                } else {
                    RETRY
                }
            }
            // Refused when it's set; only an old setting can get here.
            None => {
                console_log!("sntp: \"{server}\" isn't a time server");
                KE_RETRY
            }
        };
        Timer::after(wait).await;
    }
}

/// Asks the addresses of `server` in turn (IPv6 first, see
/// [`crate::resolve`]) until one answers.
async fn query(stack: Stack<'static>, server: &str, buffers: &mut DatagramBuffers) -> bool {
    let addresses = crate::resolve::resolve(stack, server).await;
    if addresses.is_empty() {
        console_log!("sntp: cannot resolve {server}");
        return false;
    }
    let mut rx_meta = [PacketMetadata::EMPTY; 2];
    let mut tx_meta = [PacketMetadata::EMPTY; 2];
    for address in addresses {
        // A socket for each address: a request to a neighbour that doesn't
        // answer neighbour discovery stays queued in its socket, and the
        // next one would wait behind it.
        let mut socket = UdpSocket::new(
            stack,
            &mut rx_meta,
            &mut buffers.rx[..128],
            &mut tx_meta,
            &mut buffers.tx[..128],
        );
        if socket.bind(0).is_ok() && ask(&mut socket, address, server).await {
            return true;
        }
    }
    false
}

async fn ask(socket: &mut UdpSocket<'_>, address: IpAddress, server: &str) -> bool {
    // Version 4, mode 3 (client); everything else zero.
    let mut packet = [0u8; 48];
    packet[0] = 0x23;
    if socket.send_to(&packet, (address, NTP_PORT)).await.is_err() {
        return false;
    }
    let received =
        embassy_time::with_timeout(Duration::from_secs(5), socket.recv_from(&mut packet)).await;
    let Ok(Ok((48.., _))) = received else {
        return false;
    };
    // Mode 4 (server), a stratum, and a transmit timestamp.
    let stratum = packet[1];
    if packet[0] & 0x07 != 4 || stratum == 0 || stratum > 15 {
        return false;
    }
    let seconds = u64::from(u32::from_be_bytes([
        packet[40], packet[41], packet[42], packet[43],
    ]));
    let Some(unix) = seconds.checked_sub(NTP_TO_UNIX) else {
        return false;
    };
    set(unix as i64, server, false)
}

/// Sets the clock to `unix` as answered by `server`, `authenticated` by
/// NTS or not. False if that is before the firmware was built.
pub fn set(unix: i64, server: &str, authenticated: bool) -> bool {
    let protocol = if authenticated { "nts" } else { "sntp" };
    if unix < build_time() {
        console_log!("{protocol}: {server} answered a time before the firmware was built");
        return false;
    }
    let first = now().is_none();
    let base = unix - Instant::now().as_secs() as i64;
    UNIX_AT_BOOT.store(base as u32, Ordering::Relaxed);
    AUTHENTICATED.store(authenticated, Ordering::Relaxed);
    if first {
        let t = DateTime::from_unix(unix);
        console_log!(
            "{protocol}: time is {:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
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
        let assumed = ASSUMED.load(Ordering::Relaxed);
        let t = DateTime::from_unix(now().or((assumed != 0).then_some(i64::from(assumed)))?);
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
