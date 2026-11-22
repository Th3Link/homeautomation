//! The EST client (RFC 7030): sends the CSR over TLS and brings back the
//! PKCS#7 with the issued certificate. The server is always verified: its
//! certificate must chain to the trust anchor and name the URL's host;
//! there is no switch to turn that off.

use crate::https::{self, TcpStream};
use core::ffi::CStr;
use embassy_net::tcp::TcpSocket;
use embassy_net::Stack;
use embassy_time::{with_timeout, Duration};
use embedded_io::Write as _;
use embedded_io_async::Write as _;
use gateway_core::est::{self, Answer, Operation, RequestHead};
use gateway_core::x509::pem::{self, Base64};
use mbedtls_rs::{AuthMode, ClientSessionConfig, Credentials, Session, SessionConfig, TlsVersion};

/// The largest response read (a PKCS#7 with a long chain, in base64).
const RESPONSE_MAX: usize = 12 * 1024;
const _: () = assert!(RESPONSE_MAX < https::SCRATCH_SIZE);
const TIMEOUT: Duration = Duration::from_secs(30);

/// Why an EST exchange failed; `code` is what the API shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstError {
    BadUrl,
    Dns,
    Connect,
    /// The TLS handshake failed: most often the server's certificate
    /// doesn't chain to the trust anchor or doesn't name the host.
    Tls,
    Timeout,
    /// The server answered something that isn't an EST answer.
    Malformed,
    /// The server refused (an HTTP status other than 200/202).
    Refused(u16),
    /// The request awaits approval; ask again after that many seconds.
    Pending(u32),
    NoMemory,
}

impl EstError {
    pub fn code(self) -> &'static str {
        match self {
            EstError::BadUrl => "bad_url",
            EstError::Dns => "dns",
            EstError::Connect => "connect",
            EstError::Tls => "tls",
            EstError::Timeout => "timeout",
            EstError::Malformed => "malformed_answer",
            EstError::Refused(_) => "refused",
            EstError::Pending(_) => "pending",
            EstError::NoMemory => "no_memory",
        }
    }
}

pub struct Auth {
    pub trust: mbedtls_rs::Certificate<'static>,
    pub client: Option<Credentials<'static>>,
}

/// Sends `csr` (DER) to the EST server at `url` and returns the PKCS#7
/// (DER) it issued in the supplied scratch buffer.
pub async fn exchange(
    stack: Stack<'static>,
    url: &str,
    operation: Operation,
    csr: &[u8],
    auth: Auth,
    buffers: &mut https::Buffers,
    scratch: &mut [u8],
) -> Result<usize, EstError> {
    let server = est::parse_url(url).ok_or(EstError::BadUrl)?;
    let tls = https::tls().ok_or(EstError::Tls)?;

    let addresses = crate::resolve::resolve(stack, server.host).await;
    if addresses.is_empty() {
        return Err(EstError::Dns);
    }

    let mut host = heapless::String::<{ super::URL_CAP + 1 }>::try_from(server.host)
        .map_err(|_| EstError::BadUrl)?;
    host.push('\0').map_err(|_| EstError::BadUrl)?;
    let host = CStr::from_bytes_with_nul(host.as_bytes()).map_err(|_| EstError::BadUrl)?;

    let mut socket = TcpSocket::new(stack, &mut buffers.http, &mut buffers.out);
    socket.set_timeout(Some(TIMEOUT));
    match crate::resolve::connect(&mut socket, &addresses, server.port).await {
        Ok(()) => {}
        Err(crate::resolve::ConnectFailed::Timeout) => return Err(EstError::Timeout),
        Err(_) => return Err(EstError::Connect),
    }

    let mut config = ClientSessionConfig::new();
    config.ca_chain = Some(auth.trust);
    config.creds = auth.client;
    config.server_name = Some(host);
    config.auth_mode = AuthMode::Required;
    config.min_version = TlsVersion::Tls1_2;
    let config = SessionConfig::Client(config);

    let result = {
        let mut session =
            Session::new(tls, TcpStream(&mut socket), &config).map_err(|_| EstError::Tls)?;
        let result =
            with_timeout(TIMEOUT, talk(&mut session, server, operation, csr, scratch)).await;
        let _ = with_timeout(Duration::from_secs(2), session.close()).await;
        result.unwrap_or(Err(EstError::Timeout))
    };
    socket.close();
    let _ = with_timeout(Duration::from_secs(1), socket.flush()).await;
    socket.abort();
    result
}

async fn talk<T>(
    session: &mut Session<'_, T>,
    server: est::Server<'_>,
    operation: Operation,
    csr: &[u8],
    scratch: &mut [u8],
) -> Result<usize, EstError>
where
    T: embedded_io_async::Read + embedded_io_async::Write,
{
    session.connect().await.map_err(|_| EstError::Tls)?;

    let encoded = csr.len().div_ceil(3) * 4;
    let body_len = encoded + encoded.saturating_sub(1) / 64;
    let mut out = &mut *scratch;
    write!(
        out,
        "{}{}",
        RequestHead {
            server,
            operation,
            body_len
        },
        Base64 {
            bytes: csr,
            line: 64
        }
    )
    .map_err(|_| EstError::NoMemory)?;
    let remaining = out.len();
    session
        .write_all(&scratch[..scratch.len() - remaining])
        .await
        .map_err(|_| EstError::Connect)?;
    session.flush().await.map_err(|_| EstError::Connect)?;

    // Read one byte beyond the protocol limit so a full valid response can
    // be distinguished from an oversized one without another receive buffer.
    let response = &mut scratch[..=RESPONSE_MAX];
    let mut len = 0;
    loop {
        match session.read(&mut response[len..]).await {
            Ok(0) => break,
            Ok(n) => {
                len += n;
                if len > RESPONSE_MAX {
                    return Err(EstError::Malformed);
                }
            }
            Err(_) if len > 0 => break,
            Err(_) => return Err(EstError::Connect),
        }
    }

    let start = response.as_ptr() as usize;
    match est::parse_response(&mut response[..len]).map_err(|_| EstError::Malformed)? {
        Answer::Issued(base64) => {
            let der = pem::decode_base64_in_place(base64).map_err(|_| EstError::Malformed)?;
            let offset = der.as_ptr() as usize - start;
            let len = der.len();
            response.copy_within(offset..offset + len, 0);
            Ok(len)
        }
        Answer::Pending { retry_after } => Err(EstError::Pending(retry_after)),
        Answer::Refused { status } => Err(EstError::Refused(status)),
    }
}
