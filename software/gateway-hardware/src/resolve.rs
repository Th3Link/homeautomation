//! Host names to addresses, IPv6 first.
//!
//! The MQTT brokers, the EST server and the NTP server are given as a name
//! or an address. A name is looked up with AAAA and A queries; the
//! addresses come back in the order they are worth trying: IPv6 first when
//! the gateway has an IPv6 address beyond link-local (SLAAC or DHCPv6),
//! IPv4 first when it has only IPv4. A family the gateway has no address in
//! isn't asked for at all (so an IPv6-only network doesn't wait for A
//! records it can't use), unless it has none at all yet.

use embassy_net::dns::DnsQueryType;
use embassy_net::{IpAddress, Stack};

/// At most two addresses of each family (smoltcp's `DNS_MAX_RESULT_COUNT`).
pub type Addresses = heapless::Vec<IpAddress, 4>;

/// The addresses of `host` (a name, or an IPv4 or IPv6 address, the latter
/// with or without brackets), in the order to try them; empty if it doesn't
/// resolve.
pub async fn resolve(stack: Stack<'static>, host: &str) -> Addresses {
    let literal = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    if let Ok(address) = literal.parse::<IpAddress>() {
        return core::iter::once(address).collect();
    }

    let has_v6 = stack
        .ipv6_addresses()
        .iter()
        .any(|cidr| !cidr.address().is_unicast_link_local());
    let has_v4 = stack.config_v4().is_some();
    let order: &[DnsQueryType] = match (has_v6, has_v4) {
        (true, true) => &[DnsQueryType::Aaaa, DnsQueryType::A],
        (true, false) => &[DnsQueryType::Aaaa],
        (false, true) => &[DnsQueryType::A],
        (false, false) => &[DnsQueryType::Aaaa, DnsQueryType::A],
    };
    let mut addresses = Addresses::new();
    for &query_type in order {
        // On the heap: the DNS query's future is large and only needed for
        // a moment.
        if let Ok(found) = crate::heap::boxed(|| stack.dns_query(host, query_type)).await {
            for address in found {
                let _ = addresses.push(address);
            }
        }
    }
    addresses
}

/// How long one address gets to accept a TCP connection before the next
/// one is tried.
pub const CONNECT_TIMEOUT: embassy_time::Duration = embassy_time::Duration::from_secs(10);

/// Why [`connect`] found no address that accepted the connection: what
/// happened with the last one tried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectFailed {
    /// No address to try.
    NoAddress,
    /// No answer within [`CONNECT_TIMEOUT`].
    Timeout,
    /// Refused, no route, ...
    Error(embassy_net::tcp::ConnectError),
}

/// Connects `socket` to the first of `addresses` (in order) that accepts a
/// connection to `port`.
pub async fn connect(
    socket: &mut embassy_net::tcp::TcpSocket<'_>,
    addresses: &[IpAddress],
    port: u16,
) -> Result<(), ConnectFailed> {
    let mut failed = ConnectFailed::NoAddress;
    for &address in addresses {
        match embassy_time::with_timeout(CONNECT_TIMEOUT, socket.connect((address, port))).await {
            Ok(Ok(())) => return Ok(()),
            Ok(Err(e)) => failed = ConnectFailed::Error(e),
            Err(_) => failed = ConnectFailed::Timeout,
        }
        // Back to closed, for the next attempt.
        socket.abort();
    }
    Err(failed)
}
