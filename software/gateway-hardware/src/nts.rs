//! Network Time Security (RFC 8915, ADR 0023): the time, authenticated.
//! For a time source set as `nts://host[:port]` ([`crate::wall_clock`]).
//!
//! 1. **Key exchange** (NTS-KE): TLS 1.3 to the server's port 4460, ALPN
//!    `ntske/1`, its certificate checked against the trusted CAs like an
//!    MQTT broker's ([`tls::Trust::Authority`]). Before the time is known
//!    that check can't tell whether the certificate is still valid, only
//!    that it didn't expire before the firmware was built
//!    ([`Dates::NowOrBuildTime`]). It brings eight cookies and the keys
//!    (TLS exporter).
//! 2. **NTP** with the cookies: every request uses one up and asks for as
//!    many fresh ones as are missing; only an answer authenticated with the
//!    keys sets the clock.
//!
//! The key exchange is repeated when the cookies are used up, when the
//! server says it can't read them any more (NTS NAK), after
//! [`MAX_UNANSWERED`] requests in a row went unanswered, and when the
//! setting changes. It isn't kept across restarts. A server that can't be
//! reached over NTS is not asked without it.

use crate::cert::roots::Trusted;
use crate::console_log;
use crate::tls::{self, ChainProblems, Dates, Trust};
use core::cell::Cell;
use embassy_net::tcp::TcpSocket;
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::Stack;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{with_deadline, with_timeout, Duration, Instant};
use embedded_io_async::Write as _;
use esp_hal::rng::Trng;
use gateway_core::nts::{self, AnswerError, Cookies, KeError, Keys, Sent, NAME_CAP};
use heapless::String;

/// For the key exchange: connecting, the handshake, the answer.
const KE_TIMEOUT: Duration = Duration::from_secs(20);
const ANSWER_TIMEOUT: Duration = Duration::from_secs(5);
/// Requests in a row without an authenticated answer before the key
/// exchange is repeated.
const MAX_UNANSWERED: u8 = 4;
/// A request with a cookie and seven placeholders, with the ~100-byte
/// cookies servers hand out (956 bytes), and its answer.
const PACKET_CAP: usize = 1024;
/// The NTS-KE answer: eight cookies and the records around them.
const KE_ANSWER_CAP: usize = 1280;
/// Seconds from 1900 (NTP's epoch) to 1970.
const NTP_TO_UNIX: u64 = 2_208_988_800;

/// What a key exchange established.
struct Association {
    /// The setting it was made for.
    ke_host: String<NAME_CAP>,
    ke_port: u16,
    keys: Keys,
    cookies: Cookies,
    /// Where to send the NTP requests.
    ntp_host: String<NAME_CAP>,
    ntp_port: u16,
    unanswered: u8,
}

static ASSOCIATION: Mutex<CriticalSectionRawMutex, Option<Association>> = Mutex::new(None);

/// The state of NTS, for the UI.
#[derive(Debug, Clone, Copy, Default)]
pub struct Status {
    /// Cookies at hand.
    pub cookies: usize,
    /// The last successful key exchange.
    pub key_exchange: Option<Instant>,
    /// The last authenticated answer.
    pub answer: Option<Instant>,
    /// What went wrong last, if the last attempt failed.
    pub error: Option<&'static str>,
}

static STATUS: BlockingMutex<CriticalSectionRawMutex, Cell<Status>> =
    BlockingMutex::new(Cell::new(Status {
        cookies: 0,
        key_exchange: None,
        answer: None,
        error: None,
    }));

pub fn status() -> Status {
    STATUS.lock(Cell::get)
}

fn update_status(change: impl FnOnce(&mut Status)) {
    STATUS.lock(|cell| {
        let mut status = cell.get();
        change(&mut status);
        cell.set(status);
    });
}

/// What an attempt came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The clock is set, from an authenticated answer.
    Synchronised,
    /// No authenticated answer this time.
    Failed,
    /// The key exchange failed: no point asking again right away.
    KeyExchangeFailed,
}

/// Sets the clock from the NTS server `host` (NTS-KE on `port`), with a
/// key exchange first if needed.
pub async fn synchronise(stack: Stack<'static>, host: &str, port: u16) -> Outcome {
    let mut association = ASSOCIATION.lock().await;
    let usable = association
        .as_ref()
        .is_some_and(|a| a.ke_host == host && a.ke_port == port && !a.cookies.is_empty());
    if !usable {
        *association = None;
        match key_exchange(stack, host, port).await {
            Ok(established) => {
                console_log!(
                    "nts: key exchange with {host}: {} cookies, NTP at {}:{}",
                    established.cookies.len(),
                    established.ntp_host,
                    established.ntp_port
                );
                update_status(|s| {
                    s.cookies = established.cookies.len();
                    s.key_exchange = Some(Instant::now());
                    s.error = None;
                });
                *association = Some(established);
            }
            Err(why) => {
                console_log!("nts: key exchange with {host} failed: {why}");
                update_status(|s| {
                    s.cookies = 0;
                    s.error = Some(why);
                });
                return Outcome::KeyExchangeFailed;
            }
        }
    }
    let Some(established) = association.as_mut() else {
        return Outcome::Failed;
    };

    let result = crate::heap::boxed(|| query(stack, established)).await;
    let cookies = established.cookies.len();
    match result {
        Ok(unix) => {
            established.unanswered = 0;
            update_status(|s| {
                s.cookies = cookies;
                s.answer = Some(Instant::now());
                s.error = None;
            });
            let server = established.ntp_host.clone();
            drop(association);
            if crate::wall_clock::set(unix, &server, true) {
                Outcome::Synchronised
            } else {
                Outcome::Failed
            }
        }
        Err(why) => {
            established.unanswered += 1;
            if established.unanswered >= MAX_UNANSWERED {
                // Maybe the server doesn't take these cookies any more.
                established.cookies.clear();
            }
            let cookies = established.cookies.len();
            console_log!("nts: {}: {why}", established.ntp_host);
            update_status(|s| {
                s.cookies = cookies;
                s.error = Some(why);
            });
            Outcome::Failed
        }
    }
}

/// NTS-KE with `host` on `port`.
async fn key_exchange(
    stack: Stack<'static>,
    host: &str,
    port: u16,
) -> Result<Association, &'static str> {
    let roots = Trusted::load().await;
    if roots.is_empty() {
        return Err("no trusted certificate authority to check the server against");
    }
    let addresses = crate::resolve::resolve(stack, host).await;
    if addresses.is_empty() {
        return Err("cannot resolve the name");
    }

    // On the heap, for the moment it takes. The handshake's messages and
    // the request are small, the answer is under a kilobyte; only the
    // server's flight with its certificates needs a large record buffer.
    let mut rx = alloc::vec![0u8; 1024];
    let mut tx = alloc::vec![0u8; 512];
    let mut socket = TcpSocket::new(stack, &mut rx, &mut tx);
    socket.set_timeout(Some(KE_TIMEOUT));
    if let Err(e) = crate::resolve::connect(&mut socket, &addresses, port).await {
        console_log!("nts: {host}:{port}: {e:?}");
        return Err("cannot connect");
    }

    // One TLS session at a time, with the web server and the EST client:
    // the handshake's future, its buffers and the certificate check are
    // another ~25 KB of heap. Taken only now, so that a server that doesn't
    // answer doesn't hold up the web server.
    let _slot = crate::https::slot().await;
    crate::heap::boxed(|| handshake(socket, &roots, host, port)).await
}

/// NTS-KE over the connected `socket`: TLS, the request, the answer, the
/// keys.
async fn handshake(
    socket: TcpSocket<'_>,
    roots: &Trusted,
    host: &str,
    port: u16,
) -> Result<Association, &'static str> {
    let mut read_record = alloc::vec![0u8; tls::READ_RECORD_BYTES];
    let mut write_record = alloc::vec![0u8; 1024];

    let mut problems = 0;
    let trust = Trust::Authority {
        roots,
        host,
        dates: Dates::NowOrBuildTime,
        problems: &mut problems,
    };
    const ALPN: &[&[u8]] = &[nts::ALPN];
    let opened = with_timeout(
        KE_TIMEOUT,
        tls::open_with_alpn(socket, &mut read_record, &mut write_record, trust, ALPN),
    )
    .await;
    let mut connection = match opened {
        Ok(Ok(connection)) => connection,
        Ok(Err(e)) => {
            if problems != 0 {
                console_log!("nts: {host}: certificate {}", ChainProblems(problems));
                return Err("the server's certificate isn't trusted");
            }
            console_log!("nts: {host}: TLS handshake failed: {e:?}");
            return Err("TLS handshake failed");
        }
        Err(_) => return Err("no answer (TLS handshake)"),
    };

    let deadline = Instant::now() + KE_TIMEOUT;
    let sent = async {
        connection.write_all(&nts::KE_REQUEST).await?;
        connection.flush().await
    };
    match with_deadline(deadline, sent).await {
        Ok(Ok(())) => {}
        Ok(Err(_)) => return Err("connection lost"),
        Err(_) => return Err("no answer"),
    }
    let mut answer = alloc::vec![0u8; KE_ANSWER_CAP];
    let mut len = 0;
    let response = loop {
        let read = match with_deadline(deadline, connection.read(&mut answer[len..])).await {
            Ok(Ok(read)) => read,
            Ok(Err(_)) => 0,
            Err(_) => return Err("no answer"),
        };
        len += read;
        match nts::parse_ke_response(&answer[..len]) {
            Err(KeError::Incomplete) if read > 0 && len < answer.len() => continue,
            Err(KeError::Incomplete) => return Err("incomplete answer"),
            Err(KeError::Error(_)) => return Err("the server refused"),
            Err(KeError::NoNtpv4 | KeError::NoAead) => {
                return Err("the server doesn't offer NTPv4 with AES-SIV")
            }
            Err(KeError::NoCookies) => return Err("the server sent no cookies"),
            Err(_) => return Err("malformed answer"),
            Ok(response) => break response,
        }
    };

    let mut keys = Keys {
        c2s: [0; nts::KEY_LEN],
        s2c: [0; nts::KEY_LEN],
    };
    for (key, server_to_client) in [(&mut keys.c2s, false), (&mut keys.s2c, true)] {
        connection
            .export_keying_material(
                nts::EXPORTER_LABEL,
                &nts::exporter_context(server_to_client),
                key,
            )
            .map_err(|_| "no keys")?;
    }
    let _ = with_deadline(deadline, connection.close()).await;

    let ntp_host = match response.server {
        Some(server) => server,
        None => String::try_from(host).map_err(|_| "name too long")?,
    };
    Ok(Association {
        ke_host: String::try_from(host).map_err(|_| "name too long")?,
        ke_port: port,
        keys,
        cookies: response.cookies,
        ntp_host,
        ntp_port: response.port.unwrap_or(nts::NTP_PORT),
        unanswered: 0,
    })
}

/// Asks the NTP server of `association` (each address in turn, a cookie
/// each) until one answers; the time in Unix seconds.
async fn query(stack: Stack<'static>, association: &mut Association) -> Result<i64, &'static str> {
    let addresses = crate::resolve::resolve(stack, &association.ntp_host).await;
    if addresses.is_empty() {
        return Err("cannot resolve the NTP server");
    }
    let rng = Trng::try_new().map_err(|_| "no random numbers")?;
    let mut rx_meta = [PacketMetadata::EMPTY; 2];
    let mut tx_meta = [PacketMetadata::EMPTY; 2];
    let mut rx = alloc::vec![0u8; PACKET_CAP];
    let mut tx = alloc::vec![0u8; PACKET_CAP];
    let mut socket = UdpSocket::new(stack, &mut rx_meta, &mut rx, &mut tx_meta, &mut tx);
    socket.bind(0).map_err(|_| "no socket")?;
    let mut packet = alloc::vec![0u8; PACKET_CAP];

    let mut failed = "no answer";
    for address in addresses {
        let Some(cookie) = association.cookies.take() else {
            return Err("out of cookies");
        };
        let mut sent = Sent {
            unique_id: [0; nts::UNIQUE_ID_LEN],
            transmit: [0; 8],
        };
        let mut nonce = [0u8; nts::NONCE_LEN];
        rng.read(&mut sent.unique_id);
        rng.read(&mut sent.transmit);
        rng.read(&mut nonce);
        // Ask for as many fresh cookies as are missing.
        let wanted = nts::MAX_COOKIES - association.cookies.len() - 1;
        let len = nts::request(
            &mut packet,
            &association.keys,
            &cookie,
            wanted,
            &sent,
            &nonce,
        )
        .ok_or("cookie too long")?;
        if socket
            .send_to(&packet[..len], (address, association.ntp_port))
            .await
            .is_err()
        {
            continue;
        }

        // Anything that isn't the answer (forged, or to another request)
        // is passed over until it comes or the time is up.
        let deadline = Instant::now() + ANSWER_TIMEOUT;
        while let Ok(Ok((received, _))) =
            with_deadline(deadline, socket.recv_from(&mut packet)).await
        {
            match nts::parse_response(
                &packet[..received],
                &association.keys,
                &sent,
                &mut association.cookies,
            ) {
                Ok(answer) => {
                    let seconds = answer.transmit >> 32;
                    return seconds
                        .checked_sub(NTP_TO_UNIX)
                        .map(|unix| unix as i64)
                        .ok_or("the server's time is out of range");
                }
                Err(AnswerError::Nak) => {
                    association.cookies.clear();
                    return Err("the server can't read the cookies any more (NTS NAK)");
                }
                Err(AnswerError::Unusable) => {
                    failed = "the server isn't synchronised";
                    break;
                }
                Err(AnswerError::Unauthenticated) => failed = "answer not authenticated",
                Err(AnswerError::NotOurs | AnswerError::Malformed) => {}
            }
        }
    }
    Err(failed)
}
