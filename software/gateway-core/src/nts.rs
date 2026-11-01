//! Network Time Security for NTP (RFC 8915): the wire formats.
//!
//! NTS has two parts:
//!
//! 1. **NTS-KE** (key establishment), once in a while: TLS 1.3 to port
//!    [`KE_PORT`] with ALPN [`ALPN`]. The client sends [`KE_REQUEST`]; the
//!    server answers with cookies and, optionally, the NTP server and port
//!    to use ([`parse_ke_response`]). Both sides take two keys from the TLS
//!    exporter ([`EXPORTER_LABEL`], [`exporter_context`]): client to server
//!    and server to client.
//! 2. **NTP with extension fields**, every time the clock is set: the
//!    request ([`request`]) carries a unique identifier, one cookie and an
//!    authenticator (AES-SIV with the client-to-server key over the whole
//!    packet). The answer ([`parse_response`]) has to echo the identifier
//!    and be authenticated with the server-to-client key; it brings fresh
//!    cookies, encrypted.
//!
//! A cookie is used once. The server keeps no state: the cookie carries the
//! keys, encrypted for the server alone.
//!
//! Only AEAD_AES_SIV_CMAC_256 (the one every server has to support) and
//! NTPv4. Pure: the caller does the TLS, the UDP and the random numbers.

use aes_siv::aead::KeyInit;
use aes_siv::siv::Aes128Siv;
use aes_siv::Tag;
use heapless::{String, Vec};

/// The NTS-KE server's default TCP port.
pub const KE_PORT: u16 = 4460;
/// The TLS ALPN protocol of NTS-KE.
pub const ALPN: &[u8] = b"ntske/1";
/// The TLS exporter label both keys are taken with.
pub const EXPORTER_LABEL: &[u8] = b"EXPORTER-network-time-security";
/// NTP's default port, unless NTS-KE names another one.
pub const NTP_PORT: u16 = 123;
/// Length of each key: AES-SIV-CMAC-256 takes two AES-128 keys.
pub const KEY_LEN: usize = 32;

/// NTS-KE "Next Protocol": NTPv4.
const PROTOCOL_NTPV4: u16 = 0;
/// AEAD_AES_SIV_CMAC_256 (RFC 5297).
const AEAD_AES_SIV_CMAC_256: u16 = 15;

/// The exporter context of the client-to-server key (`false`) or the
/// server-to-client key (`true`): protocol, AEAD algorithm, direction.
pub fn exporter_context(server_to_client: bool) -> [u8; 5] {
    let [p0, p1] = PROTOCOL_NTPV4.to_be_bytes();
    let [a0, a1] = AEAD_AES_SIV_CMAC_256.to_be_bytes();
    [p0, p1, a0, a1, u8::from(server_to_client)]
}

/// The keys of an NTS association.
#[derive(Clone, PartialEq, Eq)]
pub struct Keys {
    pub c2s: [u8; KEY_LEN],
    pub s2c: [u8; KEY_LEN],
}

impl core::fmt::Debug for Keys {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Keys { .. }")
    }
}

// NTS-KE records (RFC 8915 4): critical bit and type, body length, body.
const CRITICAL: u16 = 0x8000;
const END_OF_MESSAGE: u16 = 0;
const NEXT_PROTOCOL: u16 = 1;
const ERROR: u16 = 2;
const WARNING: u16 = 3;
const AEAD_ALGORITHM: u16 = 4;
const NEW_COOKIE: u16 = 5;
const SERVER: u16 = 6;
const PORT: u16 = 7;

/// What the client sends on the NTS-KE connection: NTPv4,
/// AEAD_AES_SIV_CMAC_256, end.
pub const KE_REQUEST: [u8; 16] = [
    0x80, 0x01, 0x00, 0x02, 0x00, 0x00, // next protocol: NTPv4
    0x80, 0x04, 0x00, 0x02, 0x00, 0x0F, // AEAD algorithm: AES-SIV-CMAC-256
    0x80, 0x00, 0x00, 0x00, // end of message
];

/// Most cookies kept: what servers hand out at once.
pub const MAX_COOKIES: usize = 8;
/// Largest cookie kept. Servers' cookies are around 100 bytes.
pub const COOKIE_CAP: usize = 192;
/// Longest NTP server name NTS-KE may name.
pub const NAME_CAP: usize = 64;

pub type Cookie = Vec<u8, COOKIE_CAP>;

/// The cookies at hand, each to be used once.
#[derive(Debug, Default, Clone)]
pub struct Cookies(Vec<Cookie, MAX_COOKIES>);

impl Cookies {
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Keeps `cookie`; false if there's no room or it's too long.
    pub fn push(&mut self, cookie: &[u8]) -> bool {
        match Cookie::from_slice(cookie) {
            Ok(cookie) if !cookie.is_empty() => self.0.push(cookie).is_ok(),
            _ => false,
        }
    }

    /// The oldest cookie, removed.
    pub fn take(&mut self) -> Option<Cookie> {
        (!self.0.is_empty()).then(|| self.0.remove(0))
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

/// What NTS-KE established.
#[derive(Debug, Clone)]
pub struct KeResponse {
    pub cookies: Cookies,
    /// The NTP server to ask, if not the NTS-KE server itself.
    pub server: Option<String<NAME_CAP>>,
    /// The NTP port, if not [`NTP_PORT`].
    pub port: Option<u16>,
}

/// Why an NTS-KE answer isn't usable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeError {
    /// It doesn't end yet: read more.
    Incomplete,
    Malformed,
    /// The server sent an error record (0 unrecognised critical record,
    /// 1 bad request, 2 internal server error).
    Error(u16),
    /// The server sent a warning; none is defined, so it isn't understood.
    Warning(u16),
    /// A critical record this client doesn't know.
    UnknownCritical(u16),
    /// The server doesn't speak NTPv4 with NTS.
    NoNtpv4,
    /// The server doesn't support AES-SIV-CMAC-256.
    NoAead,
    NoCookies,
}

/// Reads the server's NTS-KE answer, as far as it has arrived.
/// [`KeError::Incomplete`] until the end-of-message record is in `data`.
pub fn parse_ke_response(mut data: &[u8]) -> Result<KeResponse, KeError> {
    let mut response = KeResponse {
        cookies: Cookies::new(),
        server: None,
        port: None,
    };
    let (mut ntpv4, mut aead) = (false, false);
    loop {
        if data.len() < 4 {
            return Err(KeError::Incomplete);
        }
        let kind = u16::from_be_bytes([data[0], data[1]]);
        let length = usize::from(u16::from_be_bytes([data[2], data[3]]));
        let Some(body) = data.get(4..4 + length) else {
            return Err(KeError::Incomplete);
        };
        data = &data[4 + length..];
        let critical = kind & CRITICAL != 0;
        match kind & !CRITICAL {
            END_OF_MESSAGE => break,
            NEXT_PROTOCOL => ntpv4 = u16s(body)?.any(|p| p == PROTOCOL_NTPV4),
            ERROR => return Err(KeError::Error(u16s(body)?.next().unwrap_or(u16::MAX))),
            WARNING => return Err(KeError::Warning(u16s(body)?.next().unwrap_or(u16::MAX))),
            AEAD_ALGORITHM => aead = u16s(body)?.any(|a| a == AEAD_AES_SIV_CMAC_256),
            NEW_COOKIE => {
                // More than fit are of no use.
                let _ = response.cookies.push(body);
            }
            SERVER => {
                let name = core::str::from_utf8(body).map_err(|_| KeError::Malformed)?;
                if name.is_empty() || !name.is_ascii() {
                    return Err(KeError::Malformed);
                }
                response.server = Some(String::try_from(name).map_err(|_| KeError::Malformed)?);
            }
            PORT => {
                let &[high, low] = body else {
                    return Err(KeError::Malformed);
                };
                response.port = Some(u16::from_be_bytes([high, low]));
            }
            other if critical => return Err(KeError::UnknownCritical(other)),
            _ => {}
        }
    }
    if !ntpv4 {
        return Err(KeError::NoNtpv4);
    }
    if !aead {
        return Err(KeError::NoAead);
    }
    if response.cookies.is_empty() {
        return Err(KeError::NoCookies);
    }
    Ok(response)
}

/// A record body of 16-bit values.
fn u16s(body: &[u8]) -> Result<impl Iterator<Item = u16> + '_, KeError> {
    if !body.len().is_multiple_of(2) {
        return Err(KeError::Malformed);
    }
    Ok(body
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&pair| u16::from_be_bytes(pair)))
}

// NTP extension fields (RFC 7822, RFC 8915 5): type, length (with the
// header, a multiple of 4), body.
const UNIQUE_IDENTIFIER: u16 = 0x0104;
const NTS_COOKIE: u16 = 0x0204;
const COOKIE_PLACEHOLDER: u16 = 0x0304;
const AUTHENTICATOR: u16 = 0x0404;

/// The NTP header's length.
const HEADER: usize = 48;
/// Length of the unique identifier and of the authenticator's nonce.
pub const UNIQUE_ID_LEN: usize = 32;
pub const NONCE_LEN: usize = 16;

/// What an answer has to match: the request's unique identifier and
/// transmit timestamp (random, so they don't tell the local time).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sent {
    pub unique_id: [u8; UNIQUE_ID_LEN],
    pub transmit: [u8; 8],
}

/// Builds an NTS-protected NTP request in `out`: the header, the unique
/// identifier, `cookie`, up to `placeholders` cookie placeholders (each
/// asks for one more fresh cookie; as many as fit in `out`) and the
/// authenticator. Returns its length; `None` if not even the request
/// without placeholders fits.
pub fn request(
    out: &mut [u8],
    keys: &Keys,
    cookie: &[u8],
    placeholders: usize,
    sent: &Sent,
    nonce: &[u8; NONCE_LEN],
) -> Option<usize> {
    let cookie_field = 4 + padded(cookie.len());
    let authenticator = 4 + 4 + NONCE_LEN + 16;
    let fixed = HEADER + 4 + UNIQUE_ID_LEN + cookie_field + authenticator;
    let room = out.len().checked_sub(fixed)?;
    let placeholders = placeholders.min(room / cookie_field);

    out[..HEADER].fill(0);
    // LI 0, version 4, mode 3 (client).
    out[0] = 0x23;
    out[40..HEADER].copy_from_slice(&sent.transmit);
    let mut at = HEADER;
    at = field(out, at, UNIQUE_IDENTIFIER, &sent.unique_id);
    at = field(out, at, NTS_COOKIE, cookie);
    for _ in 0..placeholders {
        let end = at + cookie_field;
        out[at..end].fill(0);
        out[at..at + 2].copy_from_slice(&COOKIE_PLACEHOLDER.to_be_bytes());
        out[at + 2..at + 4].copy_from_slice(&(cookie_field as u16).to_be_bytes());
        at = end;
    }

    // Authenticator: nonce length, ciphertext length, nonce, ciphertext
    // (the SIV tag alone: nothing is encrypted).
    let mut siv = Aes128Siv::new((&keys.c2s).into());
    let tag = siv
        .encrypt_in_place_detached([&out[..at], &nonce[..]], &mut [])
        .ok()?;
    let mut body = [0u8; 4 + NONCE_LEN + 16];
    body[..2].copy_from_slice(&(NONCE_LEN as u16).to_be_bytes());
    body[2..4].copy_from_slice(&16u16.to_be_bytes());
    body[4..4 + NONCE_LEN].copy_from_slice(nonce);
    body[4 + NONCE_LEN..].copy_from_slice(&tag);
    Some(field(out, at, AUTHENTICATOR, &body))
}

/// Writes one extension field at `at`, zero-padded to four bytes; returns
/// where it ends.
fn field(out: &mut [u8], at: usize, kind: u16, body: &[u8]) -> usize {
    let length = 4 + padded(body.len());
    out[at..at + 2].copy_from_slice(&kind.to_be_bytes());
    out[at + 2..at + 4].copy_from_slice(&(length as u16).to_be_bytes());
    out[at + 4..at + 4 + body.len()].copy_from_slice(body);
    out[at + 4 + body.len()..at + length].fill(0);
    at + length
}

fn padded(length: usize) -> usize {
    length.div_ceil(4) * 4
}

/// An authenticated answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Answer {
    pub stratum: u8,
    /// The server's transmit timestamp (NTP format: seconds since 1900 in
    /// the upper 32 bits).
    pub transmit: u64,
    /// How many fresh cookies it brought.
    pub cookies: usize,
}

/// Why an answer isn't taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnswerError {
    /// Not an answer to the request sent (wrong mode, timestamp or
    /// identifier): wait for another one.
    NotOurs,
    Malformed,
    /// "NTS NAK" (kiss code `NTSN`): the server can't read the cookie, a
    /// new NTS-KE is needed. Not authenticated, like every kiss code.
    Nak,
    /// Another kiss code, or an unsynchronised server.
    Unusable,
    /// No authenticator, or it doesn't verify.
    Unauthenticated,
}

/// Checks an answer to the request `sent` and authenticates it with the
/// server-to-client key; keeps the fresh cookies it brings in `cookies`.
pub fn parse_response(
    packet: &[u8],
    keys: &Keys,
    sent: &Sent,
    cookies: &mut Cookies,
) -> Result<Answer, AnswerError> {
    if packet.len() < HEADER {
        return Err(AnswerError::Malformed);
    }
    // Mode 4 (server), and an answer to this request.
    if packet[0] & 0x07 != 4 || packet[24..32] != sent.transmit {
        return Err(AnswerError::NotOurs);
    }

    let mut identified = false;
    let mut authenticated = None;
    let mut at = HEADER;
    while at < packet.len() {
        let Some(&[k0, k1, l0, l1]) = packet.get(at..at + 4) else {
            return Err(AnswerError::Malformed);
        };
        let kind = u16::from_be_bytes([k0, k1]);
        let length = usize::from(u16::from_be_bytes([l0, l1]));
        if length < 4 || !length.is_multiple_of(4) {
            return Err(AnswerError::Malformed);
        }
        let body = packet
            .get(at + 4..at + length)
            .ok_or(AnswerError::Malformed)?;
        match kind {
            UNIQUE_IDENTIFIER => identified = body == sent.unique_id,
            AUTHENTICATOR => {
                if !identified {
                    return Err(AnswerError::NotOurs);
                }
                authenticated = Some(authenticate(&packet[..at], body, keys, cookies)?);
                // Whatever follows isn't authenticated.
                break;
            }
            _ => {}
        }
        at += length;
    }
    if !identified {
        return Err(AnswerError::NotOurs);
    }

    let stratum = packet[1];
    if stratum == 0 {
        return Err(if packet[12..16] == *b"NTSN" {
            AnswerError::Nak
        } else {
            AnswerError::Unusable
        });
    }
    let cookies = authenticated.ok_or(AnswerError::Unauthenticated)?;
    // Leap indicator 3: the server's clock isn't synchronised.
    if packet[0] >> 6 == 3 || stratum > 15 {
        return Err(AnswerError::Unusable);
    }
    let transmit = u64::from_be_bytes(packet[40..48].try_into().unwrap_or_default());
    if transmit == 0 {
        return Err(AnswerError::Unusable);
    }
    Ok(Answer {
        stratum,
        transmit,
        cookies,
    })
}

/// Most bytes of encrypted extension fields an answer may carry: the
/// cookies of a full request.
const PLAINTEXT_CAP: usize = MAX_COOKIES * (4 + COOKIE_CAP);

/// Verifies the authenticator `body` over `associated` (the packet before
/// it), decrypts what it carries and keeps the cookies in it; returns how
/// many.
fn authenticate(
    associated: &[u8],
    body: &[u8],
    keys: &Keys,
    cookies: &mut Cookies,
) -> Result<usize, AnswerError> {
    let Some(&[n0, n1, c0, c1]) = body.get(..4) else {
        return Err(AnswerError::Malformed);
    };
    let nonce_len = usize::from(u16::from_be_bytes([n0, n1]));
    let ciphertext_len = usize::from(u16::from_be_bytes([c0, c1]));
    let nonce = body.get(4..4 + nonce_len).ok_or(AnswerError::Malformed)?;
    let start = 4 + padded(nonce_len);
    let ciphertext = body
        .get(start..start + ciphertext_len)
        .ok_or(AnswerError::Malformed)?;
    if ciphertext.len() < 16 || ciphertext.len() - 16 > PLAINTEXT_CAP {
        return Err(AnswerError::Malformed);
    }
    let (tag, encrypted) = ciphertext.split_at(16);
    let mut plaintext: Vec<u8, PLAINTEXT_CAP> =
        Vec::from_slice(encrypted).map_err(|_| AnswerError::Malformed)?;
    let mut siv = Aes128Siv::new((&keys.s2c).into());
    siv.decrypt_in_place_detached([associated, nonce], &mut plaintext, Tag::from_slice(tag))
        .map_err(|_| AnswerError::Unauthenticated)?;

    // The encrypted extension fields: the fresh cookies.
    let mut fresh = 0;
    let mut rest = &plaintext[..];
    while let Some(&[k0, k1, l0, l1]) = rest.get(..4) {
        let length = usize::from(u16::from_be_bytes([l0, l1]));
        let Some(field) = rest.get(4..length.max(4)) else {
            break;
        };
        if u16::from_be_bytes([k0, k1]) == NTS_COOKIE && cookies.push(field) {
            fresh += 1;
        }
        rest = &rest[length.max(4)..];
    }
    Ok(fresh)
}

/// The time source setting: a plain NTP server, or `nts://host[:port]`
/// for NTS (the port is NTS-KE's).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Server<'a> {
    Plain(&'a str),
    Nts { host: &'a str, port: u16 },
}

/// Reads a time source setting; `None` if it's an `nts://` one that
/// doesn't parse.
pub fn parse_server(setting: &str) -> Option<Server<'_>> {
    let Some(rest) = setting.strip_prefix("nts://") else {
        return Some(Server::Plain(setting));
    };
    let rest = rest.strip_suffix('/').unwrap_or(rest);
    let (host, port) = if let Some(bracketed) = rest.strip_prefix('[') {
        // An IPv6 address.
        let (host, after) = bracketed.split_once(']')?;
        match after {
            "" => (host, None),
            _ => (host, Some(after.strip_prefix(':')?)),
        }
    } else {
        match rest.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (rest, None),
        }
    };
    if host.is_empty() || host.contains(['/', '@', ' ']) {
        return None;
    }
    let port = match port {
        Some(port) => port.parse().ok().filter(|&p| p != 0)?,
        None => KE_PORT,
    };
    Some(Server::Nts { host, port })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::x509::fixtures::unhex;
    extern crate std;

    #[test]
    fn aes_siv_rfc5297_a1() {
        // RFC 5297 A.1, deterministic: one header, no nonce.
        let key: [u8; 32] =
            unhex("fffefdfcfbfaf9f8f7f6f5f4f3f2f1f0f0f1f2f3f4f5f6f7f8f9fafbfcfdfeff")
                .try_into()
                .unwrap();
        let ad = unhex("101112131415161718191a1b1c1d1e1f2021222324252627");
        let mut text = unhex("112233445566778899aabbccddee");
        let tag = Aes128Siv::new((&key).into())
            .encrypt_in_place_detached([&ad[..]], &mut text)
            .unwrap();
        assert_eq!(tag.as_slice(), unhex("85632d07c6e8f37f950acd320a2ecc93"));
        assert_eq!(text, unhex("40c02b9690c4dc04daef7f6afe5c"));
    }

    #[test]
    fn exporter_contexts() {
        assert_eq!(exporter_context(false), [0, 0, 0, 15, 0]);
        assert_eq!(exporter_context(true), [0, 0, 0, 15, 1]);
    }

    /// A KE answer as chrony sends it: protocol, AEAD, two cookies, end.
    fn ke_answer(extra: &[u8]) -> std::vec::Vec<u8> {
        let mut a = std::vec![0x80, 1, 0, 2, 0, 0, 0x80, 4, 0, 2, 0, 15];
        for fill in [0xAA, 0xBB] {
            a.extend_from_slice(&[0, 5, 0, 8]);
            a.extend_from_slice(&[fill; 8]);
        }
        a.extend_from_slice(extra);
        a.extend_from_slice(&[0x80, 0, 0, 0]);
        a
    }

    #[test]
    fn ke_response() {
        // Server, port, and an unknown record that isn't critical.
        let extra = [
            &[0, 6, 0, 11][..],
            b"ntp.example",
            &[0, 7, 0, 2, 0x01, 0x7B],
            &[0x40, 0x99, 0, 1, 7],
        ]
        .concat();
        let r = parse_ke_response(&ke_answer(&extra)).unwrap();
        assert_eq!(r.cookies.len(), 2);
        assert_eq!(r.server.as_deref(), Some("ntp.example"));
        assert_eq!(r.port, Some(379));
        let mut cookies = r.cookies;
        assert_eq!(cookies.take().unwrap()[..], [0xAA; 8]);
        assert_eq!(cookies.take().unwrap()[..], [0xBB; 8]);
        assert!(cookies.take().is_none());

        let plain = parse_ke_response(&ke_answer(&[])).unwrap();
        assert_eq!((plain.server, plain.port), (None, None));
    }

    #[test]
    fn ke_response_problems() {
        let whole = ke_answer(&[]);
        for cut in 0..whole.len() {
            assert_eq!(
                parse_ke_response(&whole[..cut]).unwrap_err(),
                KeError::Incomplete
            );
        }
        assert_eq!(
            parse_ke_response(&[0x80, 2, 0, 2, 0, 1, 0x80, 0, 0, 0]).unwrap_err(),
            KeError::Error(1)
        );
        assert_eq!(
            parse_ke_response(&ke_answer(&[0x80, 0x55, 0, 0])).unwrap_err(),
            KeError::UnknownCritical(0x55)
        );
        // No AEAD the client asked for.
        let mut other = ke_answer(&[]);
        other[11] = 16;
        assert_eq!(parse_ke_response(&other).unwrap_err(), KeError::NoAead);
        // No cookies.
        let bare = [0x80, 1, 0, 2, 0, 0, 0x80, 4, 0, 2, 0, 15, 0x80, 0, 0, 0];
        assert_eq!(parse_ke_response(&bare).unwrap_err(), KeError::NoCookies);
        // The protocol refused (empty list).
        let refused = [0x80, 1, 0, 0, 0x80, 0, 0, 0];
        assert_eq!(parse_ke_response(&refused).unwrap_err(), KeError::NoNtpv4);
    }

    const KEYS: Keys = Keys {
        c2s: [0x11; 32],
        s2c: [0x22; 32],
    };
    const SENT: Sent = Sent {
        unique_id: [0x5A; 32],
        transmit: [1, 2, 3, 4, 5, 6, 7, 8],
    };

    /// What a server does with a request: checks the authenticator with
    /// the client-to-server key, finds the cookie and the placeholders.
    fn serve(request: &[u8]) -> (usize, usize) {
        let (mut cookies, mut placeholders) = (0, 0);
        let mut at = HEADER;
        while at < request.len() {
            let kind = u16::from_be_bytes([request[at], request[at + 1]]);
            let length = usize::from(u16::from_be_bytes([request[at + 2], request[at + 3]]));
            let body = &request[at + 4..at + length];
            match kind {
                NTS_COOKIE => cookies += 1,
                COOKIE_PLACEHOLDER => placeholders += 1,
                AUTHENTICATOR => {
                    assert_eq!(at + length, request.len(), "authenticator last");
                    let nonce = &body[4..20];
                    let tag = Tag::from_slice(&body[20..36]);
                    Aes128Siv::new((&KEYS.c2s).into())
                        .decrypt_in_place_detached([&request[..at], nonce], &mut [], tag)
                        .expect("authenticator verifies");
                }
                _ => {}
            }
            at += length;
        }
        (cookies, placeholders)
    }

    /// A server's answer to `SENT`, with `fresh` cookies, authenticated
    /// with `key`.
    fn answer(fresh: usize, key: &[u8; 32]) -> std::vec::Vec<u8> {
        let mut p = std::vec![0u8; HEADER];
        p[0] = 0x24; // version 4, mode 4
        p[1] = 2;
        p[24..32].copy_from_slice(&SENT.transmit);
        p[40..48].copy_from_slice(&0xEB00_0000_0000_0001u64.to_be_bytes());
        p.extend_from_slice(&[0x01, 0x04, 0, 36]);
        p.extend_from_slice(&SENT.unique_id);
        let mut plaintext = std::vec::Vec::new();
        for i in 0..fresh {
            plaintext.extend_from_slice(&[0x02, 0x04, 0, 12]);
            plaintext.extend_from_slice(&[i as u8; 8]);
        }
        let nonce = [0x77u8; 16];
        let tag = Aes128Siv::new(key.into())
            .encrypt_in_place_detached([&p[..], &nonce[..]], &mut plaintext)
            .unwrap();
        let body_len = 4 + 16 + 16 + plaintext.len();
        p.extend_from_slice(&[0x04, 0x04]);
        p.extend_from_slice(&((4 + body_len) as u16).to_be_bytes());
        p.extend_from_slice(&16u16.to_be_bytes());
        p.extend_from_slice(&((16 + plaintext.len()) as u16).to_be_bytes());
        p.extend_from_slice(&nonce);
        p.extend_from_slice(&tag);
        p.extend_from_slice(&plaintext);
        p
    }

    #[test]
    fn request_round_trip() {
        let cookie = [0xC0u8; 102];
        let mut out = [0u8; 1024];
        let len = request(&mut out, &KEYS, &cookie, 3, &SENT, &[9; 16]).unwrap();
        // Header, identifier, cookie and three placeholders padded to 104,
        // authenticator.
        assert_eq!(len, 48 + 36 + 4 * 108 + 40);
        assert_eq!(out[0], 0x23);
        assert_eq!(serve(&out[..len]), (1, 3));

        // Placeholders only as far as they fit.
        let mut small = [0u8; 48 + 36 + 108 + 40 + 120];
        let len = request(&mut small, &KEYS, &cookie, 7, &SENT, &[9; 16]).unwrap();
        assert_eq!(serve(&small[..len]), (1, 1));
        assert!(request(&mut small[..200], &KEYS, &cookie, 0, &SENT, &[9; 16]).is_none());
    }

    #[test]
    fn response_round_trip() {
        let mut cookies = Cookies::new();
        let a = parse_response(&answer(2, &KEYS.s2c), &KEYS, &SENT, &mut cookies).unwrap();
        assert_eq!(a.stratum, 2);
        assert_eq!(a.transmit >> 32, 0xEB00_0000);
        assert_eq!(a.cookies, 2);
        assert_eq!(cookies.len(), 2);
    }

    #[test]
    fn response_problems() {
        let mut cookies = Cookies::new();
        let mut check = |p: &[u8]| parse_response(p, &KEYS, &SENT, &mut cookies);

        // Signed with the wrong key, or changed on the way.
        assert_eq!(
            check(&answer(1, &KEYS.c2s)),
            Err(AnswerError::Unauthenticated)
        );
        let mut changed = answer(1, &KEYS.s2c);
        changed[41] ^= 1;
        assert_eq!(check(&changed), Err(AnswerError::Unauthenticated));
        // Another request's identifier or timestamp.
        let mut other = answer(1, &KEYS.s2c);
        other[60] ^= 1;
        assert_eq!(check(&other), Err(AnswerError::NotOurs));
        let mut other = answer(1, &KEYS.s2c);
        other[30] ^= 1;
        assert_eq!(check(&other), Err(AnswerError::NotOurs));
        // No authenticator.
        assert_eq!(
            check(&answer(1, &KEYS.s2c)[..48 + 36]),
            Err(AnswerError::Unauthenticated)
        );
        // NTS NAK: stratum 0, kiss code NTSN.
        let mut nak = answer(0, &KEYS.s2c)[..48 + 36].to_vec();
        nak[1] = 0;
        nak[12..16].copy_from_slice(b"NTSN");
        assert_eq!(check(&nak), Err(AnswerError::Nak));
        // Truncated field.
        let whole = answer(1, &KEYS.s2c);
        assert_eq!(
            check(&whole[..whole.len() - 4]),
            Err(AnswerError::Malformed)
        );
        // Nothing was kept from any of these.
        assert!(cookies.is_empty());
    }

    #[test]
    fn server_settings() {
        assert_eq!(
            parse_server("pool.ntp.org"),
            Some(Server::Plain("pool.ntp.org"))
        );
        assert_eq!(
            parse_server("nts://ptbtime1.ptb.de"),
            Some(Server::Nts {
                host: "ptbtime1.ptb.de",
                port: KE_PORT
            })
        );
        assert_eq!(
            parse_server("nts://time.example:4461/"),
            Some(Server::Nts {
                host: "time.example",
                port: 4461
            })
        );
        assert_eq!(
            parse_server("nts://[fd00::226]"),
            Some(Server::Nts {
                host: "fd00::226",
                port: KE_PORT
            })
        );
        assert_eq!(
            parse_server("nts://[fd00::226]:4460"),
            Some(Server::Nts {
                host: "fd00::226",
                port: KE_PORT
            })
        );
        for bad in [
            "nts://",
            "nts://host:",
            "nts://host:0",
            "nts://host:x",
            "nts://[::1",
            "nts://a/b",
        ] {
            assert_eq!(parse_server(bad), None, "{bad}");
        }
    }
}
