//! Async host clients. Supply trusted roots, exchange keys once, then reuse the
//! peer for NTP queries. Repeat key exchange when the peer runs out of cookies.
use crate::{
    Answer, AnswerError, KeError, Keys, Sent, Session, ALPN, KE_REQUEST, NONCE_LEN, NTP_PORT,
    UNIQUE_ID_LEN,
};
use core::ops::AsyncFnMut;
use futures_lite::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use futures_rustls::{rustls, TlsConnector};
use std::{
    format, io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    string::String,
    sync::Arc,
    time::Duration,
};

pub use rustls::RootCertStore;

#[cfg(feature = "async-net")]
pub mod async_net;
/// smol uses the same sockets and reactor as async-net.
#[cfg(feature = "smol")]
pub use self::async_net as smol;
#[cfg(feature = "tokio")]
pub mod tokio;

const KE_TIMEOUT: Duration = Duration::from_secs(20);
const QUERY_TIMEOUT: Duration = Duration::from_secs(5);

/// The authenticated NTP endpoint and cookies obtained over TLS.
pub struct Peer {
    host: String,
    port: u16,
    session: Session,
}

impl Peer {
    pub fn endpoint(&self) -> (&str, u16) {
        (&self.host, self.port)
    }

    pub fn cookies(&self) -> usize {
        self.session.cookies.len()
    }
}

async fn establish(
    stream: impl AsyncRead + AsyncWrite + Unpin,
    host: &str,
    roots: RootCertStore,
) -> io::Result<Peer> {
    let name = rustls::pki_types::ServerName::try_from(String::from(host))
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid TLS server name"))?;
    let mut config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])
    .map_err(io::Error::other)?
    .with_root_certificates(roots)
    .with_no_client_auth();
    config.alpn_protocols.push(ALPN.to_vec());
    let mut tls = TlsConnector::from(Arc::new(config))
        .connect(name, stream)
        .await?;
    if tls.get_ref().1.alpn_protocol() != Some(ALPN) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "server did not negotiate ntske/1",
        ));
    }
    tls.write_all(&KE_REQUEST).await?;
    tls.flush().await?;
    let mut packet = [0; 4096];
    let mut len = 0;
    let response = loop {
        if len == packet.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "NTS-KE response too large",
            ));
        }
        let read = tls.read(&mut packet[len..]).await?;
        if read == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        len += read;
        match crate::parse_ke_response(&packet[..len]) {
            Ok(response) => break response,
            Err(KeError::Incomplete) => continue,
            Err(e) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("NTS-KE: {e:?}"),
                ))
            }
        }
    };
    let keys = Keys::from_exporter(|label, context, key| {
        tls.get_ref()
            .1
            .export_keying_material(key, label, Some(context))
            .map(|_| ())
    })
    .map_err(io::Error::other)?;
    Ok(Peer {
        host: String::from(response.server.as_deref().unwrap_or(host)),
        port: response.port.unwrap_or(NTP_PORT),
        session: Session {
            keys,
            cookies: response.cookies,
        },
    })
}

fn local_addr(peer: SocketAddr) -> SocketAddr {
    let ip = if peer.is_ipv4() {
        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
    } else {
        IpAddr::V6(Ipv6Addr::UNSPECIFIED)
    };
    SocketAddr::new(ip, 0)
}

async fn query_addresses(
    peer: &mut Peer,
    addresses: impl Iterator<Item = SocketAddr>,
    mut query: impl AsyncFnMut(&mut Session, SocketAddr) -> io::Result<Answer>,
) -> io::Result<Answer> {
    let mut last = io::Error::new(io::ErrorKind::AddrNotAvailable, "no NTP address");
    for addr in addresses {
        if peer.cookies() == 0 {
            return Err(io::Error::other("NTS key exchange required"));
        }
        match query(&mut peer.session, addr).await {
            Ok(answer) => return Ok(answer),
            Err(err) => last = err,
        }
    }
    Err(last)
}

async fn datagram(
    session: &mut Session,
    mut send: impl AsyncFnMut(&[u8]) -> io::Result<usize>,
    mut recv: impl AsyncFnMut(&mut [u8]) -> io::Result<usize>,
) -> io::Result<Answer> {
    let mut sent = Sent {
        unique_id: [0; UNIQUE_ID_LEN],
        transmit: [0; 8],
    };
    let mut nonce = [0; NONCE_LEN];
    for field in [&mut sent.unique_id[..], &mut sent.transmit, &mut nonce] {
        getrandom::fill(field).map_err(|_| io::Error::other("randomness unavailable"))?;
    }
    let mut packet = [0; 2048];
    let len = session
        .prepare(&mut packet, &sent, &nonce)
        .map_err(|e| io::Error::other(format!("NTS request: {e:?}")))?;
    if send(&packet[..len]).await? != len {
        return Err(io::ErrorKind::WriteZero.into());
    }
    loop {
        let n = recv(&mut packet).await?;
        if n == packet.len() {
            continue;
        } // Possibly truncated datagram.
        match session.accept(&packet[..n], &sent) {
            Ok(answer) => return Ok(answer),
            Err(AnswerError::Nak) => return Err(io::Error::other("NTS key exchange required")),
            Err(AnswerError::Unusable) => {
                return Err(io::Error::other("NTP server unsynchronized"))
            }
            Err(AnswerError::NotOurs | AnswerError::Malformed | AnswerError::Unauthenticated) => {
                continue
            }
        }
    }
}
