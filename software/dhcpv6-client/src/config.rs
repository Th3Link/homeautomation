//! What the client obtained.

use crate::names::Names;
use crate::time::Instant;
use crate::wire::{self, ntp, opt, Message, Options};
use crate::Duid;
use core::net::Ipv6Addr;
use heapless::Vec;

/// An address leased from the server (IA_NA). It is a /128: which prefixes
/// are on the link only the routers say (RFC 5942).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Address {
    pub address: Ipv6Addr,
    /// From then on the address should no longer be used for new
    /// connections; [`Instant::NEVER`] if the lifetime is infinite.
    pub preferred_until: Instant,
    /// From then on the address must no longer be used.
    pub valid_until: Instant,
}

/// The configuration from a DHCPv6 server: up to `ADDRESSES` addresses,
/// `SERVERS` DNS and NTP servers (each) and `NAME_BYTES` bytes of domain
/// names (each list, in wire format: 13 bytes for `example.com`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Configuration<const ADDRESSES: usize, const SERVERS: usize, const NAME_BYTES: usize> {
    /// The server this came from.
    pub server_id: Duid,
    /// The leased addresses; empty in stateless mode.
    pub addresses: Vec<Address, ADDRESSES>,
    /// DNS servers (RFC 3646).
    pub dns_servers: Vec<Ipv6Addr, SERVERS>,
    /// The domain search list (RFC 3646).
    pub domains: Names<NAME_BYTES>,
    /// NTP servers by address (RFC 5908 unicast and multicast addresses,
    /// and the SNTP servers of RFC 4075).
    pub ntp_servers: Vec<Ipv6Addr, SERVERS>,
    /// NTP servers by name (RFC 5908).
    pub ntp_names: Names<NAME_BYTES>,
    /// When the client asks the server again: T1 (stateful) or the
    /// Information Refresh Time (stateless).
    pub renew_at: Instant,
    /// When the client asks any server, if its own doesn't answer (T2;
    /// stateless: never).
    pub rebind_at: Instant,
}

impl<const ADDRESSES: usize, const SERVERS: usize, const NAME_BYTES: usize>
    Configuration<ADDRESSES, SERVERS, NAME_BYTES>
{
    pub(crate) fn new(server_id: Duid) -> Self {
        Self {
            server_id,
            addresses: Vec::new(),
            dns_servers: Vec::new(),
            domains: Names::new(),
            ntp_servers: Vec::new(),
            ntp_names: Names::new(),
            renew_at: Instant::NEVER,
            rebind_at: Instant::NEVER,
        }
    }

    /// Takes the DNS, domain and NTP options of `message`, replacing what
    /// was there.
    pub(crate) fn take_options(&mut self, message: &Message<'_>) {
        self.dns_servers.clear();
        self.domains.clear();
        self.ntp_servers.clear();
        self.ntp_names.clear();
        for (code, data) in message.options() {
            match code {
                opt::DNS_SERVERS => push_new(&mut self.dns_servers, wire::addresses(data)),
                opt::DOMAIN_LIST => self.domains.extend_from_wire(data),
                opt::SNTP_SERVERS => push_new(&mut self.ntp_servers, wire::addresses(data)),
                opt::NTP_SERVER => {
                    for (code, data) in Options::new(data) {
                        match code {
                            ntp::SERVER_ADDRESS | ntp::MULTICAST_ADDRESS => {
                                push_new(&mut self.ntp_servers, wire::addresses(data).take(1))
                            }
                            ntp::SERVER_FQDN => self.ntp_names.push_wire(data),
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// The end of the longest valid lifetime; `None` without addresses.
    pub(crate) fn valid_until(&self) -> Option<Instant> {
        self.addresses.iter().map(|a| a.valid_until).max()
    }

    /// Drops the addresses whose valid lifetime is over; were there any?
    pub(crate) fn expire(&mut self, now: Instant) -> bool {
        let before = self.addresses.len();
        self.addresses.retain(|a| a.valid_until > now);
        self.addresses.len() != before
    }
}

/// Adds the addresses that aren't there yet, as long as there is room.
fn push_new<const N: usize>(list: &mut Vec<Ipv6Addr, N>, new: impl Iterator<Item = Ipv6Addr>) {
    for address in new {
        if !list.contains(&address) && list.push(address).is_err() {
            break;
        }
    }
}
