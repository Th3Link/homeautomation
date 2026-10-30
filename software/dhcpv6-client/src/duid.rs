//! DHCP Unique Identifiers (RFC 8415 § 11).

use core::fmt;
use heapless::Vec;

/// The longest DUID: the 2-byte type and up to 128 bytes behind it.
pub const DUID_MAX_LEN: usize = 130;

/// DUID-LL (§ 11.4).
const DUID_LL: u16 = 3;
/// Ethernet in the hardware type registry (RFC 826).
const HARDWARE_ETHERNET: u16 = 1;

/// A client's or a server's DUID, kept as the bytes on the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Duid(Vec<u8, DUID_MAX_LEN>);

impl Duid {
    /// DUID-LL from an Ethernet MAC address: the same for as long as the
    /// interface is, with no clock or storage needed (§ 11.4).
    pub fn link_layer(mac: [u8; 6]) -> Self {
        let mut bytes = Vec::new();
        // 10 bytes always fit.
        let _ = bytes.extend_from_slice(&DUID_LL.to_be_bytes());
        let _ = bytes.extend_from_slice(&HARDWARE_ETHERNET.to_be_bytes());
        let _ = bytes.extend_from_slice(&mac);
        Self(bytes)
    }

    /// Any DUID as received or configured: the type and at least one byte,
    /// at most [`DUID_MAX_LEN`] bytes.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 3 {
            return None;
        }
        Vec::from_slice(bytes).ok().map(Self)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Hex bytes separated by colons, the way `dhclient` and `dnsmasq` log them.
impl fmt::Display for Duid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, byte) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(":")?;
            }
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::string::ToString;

    #[test]
    fn link_layer_duid() {
        let duid = Duid::link_layer([0x58, 0xbf, 0x25, 0xee, 0x55, 0xfc]);
        assert_eq!(
            duid.as_bytes(),
            [0, 3, 0, 1, 0x58, 0xbf, 0x25, 0xee, 0x55, 0xfc]
        );
        assert_eq!(duid.to_string(), "00:03:00:01:58:bf:25:ee:55:fc");
    }

    #[test]
    fn from_bytes_checks_the_length() {
        assert!(Duid::from_bytes(&[0, 3]).is_none());
        assert!(Duid::from_bytes(&[0, 4, 1]).is_some());
        assert!(Duid::from_bytes(&[0; DUID_MAX_LEN]).is_some());
        assert!(Duid::from_bytes(&[0; DUID_MAX_LEN + 1]).is_none());
    }
}
