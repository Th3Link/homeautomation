//! NTS client for async-net and smol.
use super::*;
use ::async_net::{resolve, TcpStream, UdpSocket};
use async_io::Timer;
use core::future::Future;

async fn timeout<T>(limit: Duration, work: impl Future<Output = io::Result<T>>) -> io::Result<T> {
    futures_lite::future::race(work, async {
        Timer::after(limit).await;
        Err(io::ErrorKind::TimedOut.into())
    })
    .await
}

/// Authenticate the KE server and negotiate the endpoint, traffic keys and cookies.
pub async fn key_exchange(host: &str, port: u16, roots: RootCertStore) -> io::Result<Peer> {
    timeout(KE_TIMEOUT, async {
        establish(TcpStream::connect((host, port)).await?, host, roots).await
    })
    .await
}

/// Send an authenticated NTP query to the negotiated endpoint.
pub async fn query(peer: &mut Peer) -> io::Result<Answer> {
    let addrs = timeout(QUERY_TIMEOUT, resolve(peer.endpoint())).await?;
    query_addresses(peer, addrs.into_iter(), async |session, addr| {
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
        .await
    })
    .await
}
