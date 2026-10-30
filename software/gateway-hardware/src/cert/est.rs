//! The EST client (RFC 7030): sends the CSR over TLS and brings back the
//! PKCS#7 with the issued certificate. The server is always verified: its
//! certificate must chain to the trust anchor and name the URL's host;
//! there is no switch to turn that off.

use crate::https::{self, TcpStream};
use alloc::string::String;
use alloc::vec::Vec;
use core::ffi::CStr;
use core::fmt::Write as _;
use embassy_net::tcp::TcpSocket;
use embassy_net::Stack;
use embassy_time::{with_timeout, Duration};
use embedded_io_async::Write as _;
use gateway_core::est::{self, Answer, Operation, RequestHead};
use gateway_core::x509::pem::{self, Base64};
use mbedtls_rs::{AuthMode, ClientSessionConfig, Credentials, Session, SessionConfig, TlsVersion};

/// The largest response read (a PKCS#7 with a long chain, in base64).
const RESPONSE_MAX: usize = 12 * 1024;
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

/// Sends `csr` (DER) to the EST server at `url` and returns the PKCS#7
/// (DER) it issued. `trust` is the trust anchor as a NUL-terminated PEM;
/// `client` the credentials to authenticate with, if any.
pub async fn exchange(
    stack: Stack<'static>,
    url: &str,
    operation: Operation,
    csr: &[u8],
    trust: &CStr,
    client: Option<Credentials<'static>>,
) -> Result<Vec<u8>, EstError> {
    let server = est::parse_url(url).ok_or(EstError::BadUrl)?;
    let tls = https::tls().ok_or(EstError::Tls)?;
    let trust =
        mbedtls_rs::Certificate::new(mbedtls_rs::X509::PEM(trust)).map_err(|_| EstError::Tls)?;

    let addresses = crate::resolve::resolve(stack, server.host).await;
    if addresses.is_empty() {
        return Err(EstError::Dns);
    }

    let mut host = String::from(server.host);
    host.push('\0');
    let host = CStr::from_bytes_with_nul(host.as_bytes()).map_err(|_| EstError::BadUrl)?;

    let mut rx = alloc::vec![0u8; 2048];
    let mut tx = alloc::vec![0u8; 2048];
    let mut socket = TcpSocket::new(stack, &mut rx, &mut tx);
    socket.set_timeout(Some(TIMEOUT));
    match crate::resolve::connect(&mut socket, &addresses, server.port).await {
        Ok(()) => {}
        Err(crate::resolve::ConnectFailed::Timeout) => return Err(EstError::Timeout),
        Err(_) => return Err(EstError::Connect),
    }

    let mut config = ClientSessionConfig::new();
    config.ca_chain = Some(trust);
    config.creds = client;
    config.server_name = Some(host);
    config.auth_mode = AuthMode::Required;
    config.min_version = TlsVersion::Tls1_2;
    let config = SessionConfig::Client(config);

    let result = {
        let mut session =
            Session::new(tls, TcpStream(&mut socket), &config).map_err(|_| EstError::Tls)?;
        let result = with_timeout(TIMEOUT, talk(&mut session, server, operation, csr)).await;
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
) -> Result<Vec<u8>, EstError>
where
    T: embedded_io_async::Read + embedded_io_async::Write,
{
    session.connect().await.map_err(|_| EstError::Tls)?;

    let mut body = String::new();
    write!(
        body,
        "{}",
        Base64 {
            bytes: csr,
            line: 64
        }
    )
    .map_err(|_| EstError::NoMemory)?;
    let mut request = String::new();
    write!(
        request,
        "{}",
        RequestHead {
            server,
            operation,
            body_len: body.len(),
        }
    )
    .map_err(|_| EstError::NoMemory)?;
    request.push_str(&body);
    session
        .write_all(request.as_bytes())
        .await
        .map_err(|_| EstError::Connect)?;
    session.flush().await.map_err(|_| EstError::Connect)?;
    drop(request);
    drop(body);

    // `Connection: close`: the answer ends where the server closes.
    let mut response = Vec::new();
    let mut chunk = [0u8; 512];
    loop {
        match session.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => {
                if response.len() + n > RESPONSE_MAX {
                    return Err(EstError::Malformed);
                }
                response.extend_from_slice(&chunk[..n]);
            }
            // Some servers end with a TCP close instead of close_notify.
            Err(_) if !response.is_empty() => break,
            Err(_) => return Err(EstError::Connect),
        }
    }

    match est::parse_response(&mut response).map_err(|_| EstError::Malformed)? {
        Answer::Issued(base64) => {
            let mut der = alloc::vec![0u8; base64.len() * 3 / 4 + 3];
            let len = pem::decode(base64, &mut der).map_err(|_| EstError::Malformed)?;
            der.truncate(len);
            Ok(der)
        }
        Answer::Pending { retry_after } => Err(EstError::Pending(retry_after)),
        Answer::Refused { status } => Err(EstError::Refused(status)),
    }
}
