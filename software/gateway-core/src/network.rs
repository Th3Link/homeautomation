//! The network settings: how the gateway gets its IPv4 and IPv6
//! addresses. They are read at boot; a change takes effect after a
//! restart.
//!
//! - IPv4: DHCP (the default), a link-local address (169.254.x.y, derived
//!   from the MAC, RFC 3927 without its conflict detection), or an address
//!   set by hand. Up to two DNS servers: needed by hand, and with DHCP or
//!   link-local they take the place of the DHCP server's.
//! - IPv6: SLAAC on or off (off: the routers' advertisements are ignored
//!   altogether, so neither their prefixes, nor the router, nor their M/O
//!   flags are used); DHCPv6 as the routers' flags ask, always (stateful,
//!   whatever the flags), or never; and an address with a gateway by hand,
//!   always possible next to the others. The gateway is only used without
//!   SLAAC, which takes the routers from their advertisements.

use core::net::{Ipv4Addr, Ipv6Addr};
use heapless::Vec;
use serde::{Deserialize, Serialize};

/// How the gateway gets its IPv4 address.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ipv4Mode {
    #[default]
    Dhcp,
    /// 169.254.x.y, see [`link_local_v4`].
    LinkLocal,
    /// [`NetworkConfig::ipv4_address`] and the rest set by hand.
    Manual,
}

/// When the gateway runs DHCPv6.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dhcpv6Mode {
    /// As the routers' M/O flags ask (not at all without SLAAC, which
    /// reads them).
    #[default]
    Auto,
    /// Always, for an address and the other configuration.
    Enforce,
    Off,
}

/// DNS servers that can be set by hand.
pub const DNS_SERVERS: usize = 2;

/// The network settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub ipv4_mode: Ipv4Mode,
    /// With [`Ipv4Mode::Manual`].
    pub ipv4_address: Option<Ipv4Addr>,
    pub ipv4_prefix: u8,
    /// With [`Ipv4Mode::Manual`].
    pub ipv4_gateway: Option<Ipv4Addr>,
    pub dns_servers: Vec<Ipv4Addr, DNS_SERVERS>,
    pub slaac: bool,
    pub dhcpv6: Dhcpv6Mode,
    pub ipv6_address: Option<Ipv6Addr>,
    pub ipv6_prefix: u8,
    pub ipv6_gateway: Option<Ipv6Addr>,
}

impl Default for NetworkConfig {
    /// DHCP and SLAAC, with DHCPv6 where the routers ask for it.
    fn default() -> Self {
        Self {
            ipv4_mode: Ipv4Mode::Dhcp,
            ipv4_address: None,
            ipv4_prefix: 24,
            ipv4_gateway: None,
            dns_servers: Vec::new(),
            slaac: true,
            dhcpv6: Dhcpv6Mode::Auto,
            ipv6_address: None,
            ipv6_prefix: 64,
            ipv6_gateway: None,
        }
    }
}

fn usable_v4(address: Ipv4Addr) -> bool {
    !(address.is_unspecified()
        || address.is_loopback()
        || address.is_multicast()
        || address.is_broadcast())
}

fn usable_v6(address: Ipv6Addr) -> bool {
    !(address.is_unspecified() || address.is_loopback() || address.is_multicast())
}

impl NetworkConfig {
    /// Checks the settings; the error names the field.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.ipv4_mode == Ipv4Mode::Manual {
            let address = self
                .ipv4_address
                .ok_or("ipv4_address: needed with manual")?;
            if !usable_v4(address) {
                return Err("ipv4_address: not a host address");
            }
            if !(1..=32).contains(&self.ipv4_prefix) {
                return Err("ipv4_prefix: 1-32");
            }
            let mask = u32::MAX << (32 - u32::from(self.ipv4_prefix));
            let host = u32::from(address) & !mask;
            if self.ipv4_prefix < 31 && (host == 0 || host == !mask) {
                return Err("ipv4_address: the network's own or its broadcast address");
            }
            if let Some(gateway) = self.ipv4_gateway {
                if !usable_v4(gateway) || gateway == address {
                    return Err("ipv4_gateway: not a host address");
                }
                if u32::from(gateway) & mask != u32::from(address) & mask {
                    return Err("ipv4_gateway: not in the address's network");
                }
            }
            if self.dns_servers.is_empty() {
                return Err("dns_servers: at least one with manual");
            }
        }
        if self.dns_servers.iter().any(|&server| !usable_v4(server)) {
            return Err("dns_servers: not a host address");
        }
        if let Some(address) = self.ipv6_address {
            if !usable_v6(address) || address.is_unicast_link_local() {
                return Err("ipv6_address: not a global or unique local address");
            }
            if !(1..=128).contains(&self.ipv6_prefix) {
                return Err("ipv6_prefix: 1-128");
            }
        }
        if let Some(gateway) = self.ipv6_gateway {
            if !usable_v6(gateway) {
                return Err("ipv6_gateway: not a unicast address");
            }
        }
        Ok(())
    }
}

/// The IPv4 link-local address of the MAC `mac`: always the same one, in
/// 169.254.1.0-169.254.254.255 (RFC 3927 § 2.1). There is no check whether
/// another device already has it (the stack can't send ARP probes); with
/// 65024 addresses a clash on one LAN is unlikely.
pub fn link_local_v4(mac: [u8; 6]) -> Ipv4Addr {
    // FNV-1a: spreads MACs that differ in a single bit.
    let hash = mac.iter().fold(0x811c_9dc5u32, |hash, &byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    });
    let n = hash % (254 * 256) + 256;
    let [_, _, x, y] = n.to_be_bytes();
    Ipv4Addr::new(169, 254, x, y)
}

/// The format version byte in front of the stored settings.
pub const FORMAT_VERSION: u8 = 1;

/// The largest the stored settings get (every field at its longest).
pub const STORED_MAX: usize = 96;

/// Writes `config` to `buffer` as postcard behind [`FORMAT_VERSION`], and
/// returns the bytes used. `None` if it doesn't fit.
pub fn encode<'b>(config: &NetworkConfig, buffer: &'b mut [u8]) -> Option<&'b [u8]> {
    let (version, rest) = buffer.split_first_mut()?;
    *version = FORMAT_VERSION;
    let len = postcard::to_slice(config, rest).ok()?.len();
    Some(&buffer[..=len])
}

/// Reads what [`encode`] wrote. `None` for another format version or bytes
/// that don't decode.
pub fn decode(bytes: &[u8]) -> Option<NetworkConfig> {
    let (&version, rest) = bytes.split_first()?;
    if version != FORMAT_VERSION {
        return None;
    }
    postcard::from_bytes(rest).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manual() -> NetworkConfig {
        NetworkConfig {
            ipv4_mode: Ipv4Mode::Manual,
            ipv4_address: Some(Ipv4Addr::new(192, 168, 1, 20)),
            ipv4_prefix: 24,
            ipv4_gateway: Some(Ipv4Addr::new(192, 168, 1, 1)),
            dns_servers: Vec::from_slice(&[Ipv4Addr::new(192, 168, 1, 1)]).unwrap(),
            ..NetworkConfig::default()
        }
    }

    #[test]
    fn default_is_dhcp_and_slaac() {
        let config = NetworkConfig::default();
        assert_eq!(config.ipv4_mode, Ipv4Mode::Dhcp);
        assert!(config.slaac);
        assert_eq!(config.dhcpv6, Dhcpv6Mode::Auto);
        assert_eq!(config.validate(), Ok(()));
    }

    #[test]
    fn manual_needs_an_address_and_a_dns_server() {
        assert_eq!(manual().validate(), Ok(()));
        let mut config = manual();
        config.ipv4_address = None;
        assert!(config.validate().unwrap_err().starts_with("ipv4_address"));
        let mut config = manual();
        config.dns_servers.clear();
        assert!(config.validate().unwrap_err().starts_with("dns_servers"));
        // Without a gateway: only the local network.
        let mut config = manual();
        config.ipv4_gateway = None;
        assert_eq!(config.validate(), Ok(()));
    }

    #[test]
    fn manual_rejects_addresses_that_are_no_host() {
        for (address, prefix) in [
            ([192, 168, 1, 0], 24),
            ([192, 168, 1, 255], 24),
            ([0, 0, 0, 0], 24),
            ([224, 0, 0, 1], 24),
            ([127, 0, 0, 1], 8),
        ] {
            let mut config = manual();
            config.ipv4_address = Some(address.into());
            config.ipv4_prefix = prefix;
            assert!(config.validate().is_err(), "{address:?}/{prefix}");
        }
        let mut config = manual();
        config.ipv4_prefix = 33;
        assert!(config.validate().is_err());
        // /31 and /32 have no network or broadcast address.
        let mut config = manual();
        config.ipv4_address = Some(Ipv4Addr::new(10, 0, 0, 0));
        config.ipv4_prefix = 31;
        config.ipv4_gateway = Some(Ipv4Addr::new(10, 0, 0, 1));
        assert_eq!(config.validate(), Ok(()));
    }

    #[test]
    fn the_gateway_is_in_the_network() {
        let mut config = manual();
        config.ipv4_gateway = Some(Ipv4Addr::new(192, 168, 2, 1));
        assert!(config.validate().unwrap_err().starts_with("ipv4_gateway"));
        config.ipv4_prefix = 16;
        assert_eq!(config.validate(), Ok(()));
    }

    #[test]
    fn dns_servers_replace_dhcps_and_are_checked() {
        let mut config = NetworkConfig::default();
        config.dns_servers.push(Ipv4Addr::new(9, 9, 9, 9)).unwrap();
        assert_eq!(config.validate(), Ok(()));
        config
            .dns_servers
            .push(Ipv4Addr::new(255, 255, 255, 255))
            .unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn ipv6_by_hand() {
        let mut config = NetworkConfig {
            ipv6_address: Some("2001:db8::20".parse().unwrap()),
            ipv6_gateway: Some("fe80::1".parse().unwrap()),
            ..NetworkConfig::default()
        };
        assert_eq!(config.validate(), Ok(()));
        config.ipv6_prefix = 129;
        assert!(config.validate().is_err());
        config.ipv6_prefix = 64;
        config.ipv6_address = Some("fe80::20".parse().unwrap());
        assert!(config.validate().unwrap_err().starts_with("ipv6_address"));
        config.ipv6_address = Some("ff02::1".parse().unwrap());
        assert!(config.validate().is_err());
        config.ipv6_address = None;
        config.ipv6_gateway = Some(Ipv6Addr::UNSPECIFIED);
        assert!(config.validate().unwrap_err().starts_with("ipv6_gateway"));
    }

    #[test]
    fn link_local_address_is_in_the_rfc_range_and_stable() {
        let a = link_local_v4([0x02, 0x53, 0x49, 0x4d, 0x00, 0x01]);
        assert_eq!(a, link_local_v4([0x02, 0x53, 0x49, 0x4d, 0x00, 0x01]));
        assert_ne!(a, link_local_v4([0x02, 0x53, 0x49, 0x4d, 0x00, 0x02]));
        for last in 0..=255u8 {
            for second in [0u8, 0x80, 0xff] {
                let [a, b, c, _] = link_local_v4([0x24, 0x0a, 0xc4, 0x12, second, last]).octets();
                assert_eq!((a, b), (169, 254));
                assert!((1..=254).contains(&c), "169.254.{c}.x");
            }
        }
    }

    #[test]
    fn stored_form_round_trips_and_fits() {
        let mut config = manual();
        config.dns_servers.push(Ipv4Addr::new(9, 9, 9, 9)).unwrap();
        config.slaac = false;
        config.dhcpv6 = Dhcpv6Mode::Enforce;
        config.ipv6_address = Some("2001:db8:ffff:ffff:ffff:ffff:ffff:ffff".parse().unwrap());
        config.ipv6_gateway = Some("fe80::ffff:ffff:ffff:ffff".parse().unwrap());
        let mut buffer = [0u8; STORED_MAX];
        let bytes = encode(&config, &mut buffer).expect("fits");
        assert_eq!(bytes[0], FORMAT_VERSION);
        assert_eq!(decode(bytes), Some(config));
        let mut other = bytes.to_vec();
        other[0] = FORMAT_VERSION + 1;
        assert_eq!(decode(&other), None);
    }

    #[test]
    fn json_names_the_modes() {
        let mut buffer = [0u8; 512];
        let len = serde_json_core::to_slice(&manual(), &mut buffer).unwrap();
        let text = core::str::from_utf8(&buffer[..len]).unwrap();
        assert!(text.contains(r#""ipv4_mode":"manual""#), "{text}");
        assert!(text.contains(r#""ipv4_address":"192.168.1.20""#), "{text}");
        assert!(text.contains(r#""dhcpv6":"auto""#), "{text}");
        let (back, _): (NetworkConfig, _) = serde_json_core::from_str(text).unwrap();
        assert_eq!(back, manual());
    }
}
