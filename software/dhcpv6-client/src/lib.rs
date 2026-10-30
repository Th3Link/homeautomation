#![no_std]
//! A DHCPv6 client (RFC 8415) without I/O, allocator or clock of its own.
//!
//! Two modes, chosen from the routers' Router Advertisement flags
//! ([`Mode::from_router_flags`]):
//! - **stateful** (`M`): an address from the server (IA_NA: Solicit,
//!   Advertise, Request, Reply), renewed at T1 (Renew) and T2 (Rebind),
//!   given back with [`Client::release`];
//! - **stateless** (`O`, RFC 8415 § 6.1): only the other configuration
//!   (Information-request), refreshed after the Information Refresh Time.
//!
//! Both bring the DNS servers (RFC 3646), the domain search list and the
//! NTP servers (RFC 5908, and the older SNTP option of RFC 4075).
//!
//! The caller owns the UDP socket (port [`CLIENT_PORT`]) and the clock, as
//! with smoltcp's DHCPv4 socket: it passes received datagrams to
//! [`Client::receive`], sends what [`Client::poll`] writes to
//! [`ALL_DHCP_RELAY_AGENTS_AND_SERVERS`] port [`SERVER_PORT`] from its
//! link-local address, and calls `poll` again at [`Client::poll_at`]. The
//! result is [`Client::configuration`]; [`Client::configuration_changed`]
//! tells when to apply it again.
//!
//! All storage is fixed at compile time: `ADDRESSES`, `SERVERS` and
//! `NAME_BYTES` set how many addresses, servers and bytes of domain names a
//! [`Configuration`] holds, and whatever doesn't fit is left out.
//!
//! The flow follows [mozim](https://github.com/nispor/mozim)'s DHCPv6 client
//! (Linux, tokio), which has no stateless mode; this crate is written anew
//! for `no_std`.
//!
//! Not supported: temporary addresses (IA_TA), prefix delegation (IA_PD),
//! Rapid Commit, Reconfigure, authentication, relays' Unicast option (every
//! message goes to the multicast address).

#[cfg(test)]
#[macro_use]
extern crate std;

mod client;
mod config;
mod duid;
mod names;
mod rng;
mod time;
mod wire;

#[cfg(test)]
mod tests;

pub use client::{Client, ClientConfig, Mode, State};
pub use config::{Address, Configuration};
pub use duid::{Duid, DUID_MAX_LEN};
pub use names::{Name, Names};
pub use time::Instant;
pub use wire::{BufferTooSmall, ALL_DHCP_RELAY_AGENTS_AND_SERVERS, CLIENT_PORT, SERVER_PORT};
