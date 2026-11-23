//! NTS client for Tokio's I/O and time drivers.
use super::*;
use ::tokio::{
    net::{lookup_host, TcpStream, UdpSocket},
    time::timeout,
};
use tokio_util::compat::TokioAsyncReadCompatExt as _;

/// Authenticate the KE server and negotiate the endpoint, traffic keys and cookies.
pub async fn key_exchange(host: &str, port: u16, roots: RootCertStore) -> io::Result<Peer> {
    timeout(KE_TIMEOUT, async {
        establish(
            TcpStream::connect((host, port)).await?.compat(),
            host,
            roots,
        )
        .await
    })
    .await?
}

/// Send an authenticated NTP query to the negotiated endpoint.
pub async fn query(peer: &mut Peer) -> io::Result<Answer> {
    let addrs = timeout(QUERY_TIMEOUT, lookup_host((peer.host.clone(), peer.port))).await??;
    query_addresses(peer, addrs, async |session, addr| {
        timeout(QUERY_TIMEOUT, async {
            let socket = UdpSocket::bind(local_addr(addr)).await?;
            socket.connect(addr).await?;
            datagram(
                session,
                async |p| socket.send(p).await,
                async |p| socket.recv(p).await,
            )
            .await
        })
        .await?
    })
    .await
}
