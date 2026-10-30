//! The DHCPv6 message format (RFC 8415 § 8, § 21): reading what servers send
//! and writing what the client sends.

use core::net::Ipv6Addr;

/// The client's UDP port (§ 7.2).
pub const CLIENT_PORT: u16 = 546;
/// The servers' and relay agents' UDP port.
pub const SERVER_PORT: u16 = 547;
/// Where every client message goes: `ff02::1:2` (§ 7.1).
pub const ALL_DHCP_RELAY_AGENTS_AND_SERVERS: Ipv6Addr = Ipv6Addr::new(0xff02, 0, 0, 0, 0, 0, 1, 2);

/// Message types (§ 7.3).
pub(crate) mod msg {
    pub const SOLICIT: u8 = 1;
    pub const ADVERTISE: u8 = 2;
    pub const REQUEST: u8 = 3;
    pub const RENEW: u8 = 5;
    pub const REBIND: u8 = 6;
    pub const REPLY: u8 = 7;
    pub const RELEASE: u8 = 8;
    pub const INFORMATION_REQUEST: u8 = 11;
}

/// Option codes (§ 21, RFC 3646, RFC 4075, RFC 5908).
pub(crate) mod opt {
    pub const CLIENT_ID: u16 = 1;
    pub const SERVER_ID: u16 = 2;
    pub const IA_NA: u16 = 3;
    pub const IA_ADDR: u16 = 5;
    pub const ORO: u16 = 6;
    pub const PREFERENCE: u16 = 7;
    pub const ELAPSED_TIME: u16 = 8;
    pub const STATUS_CODE: u16 = 13;
    pub const DNS_SERVERS: u16 = 23;
    pub const DOMAIN_LIST: u16 = 24;
    pub const SNTP_SERVERS: u16 = 31;
    pub const INFORMATION_REFRESH_TIME: u16 = 32;
    pub const NTP_SERVER: u16 = 56;
    pub const SOL_MAX_RT: u16 = 82;
    pub const INF_MAX_RT: u16 = 83;
}

/// Suboptions of the NTP Server option (RFC 5908 § 4).
pub(crate) mod ntp {
    pub const SERVER_ADDRESS: u16 = 1;
    pub const MULTICAST_ADDRESS: u16 = 2;
    pub const SERVER_FQDN: u16 = 3;
}

/// Status codes (§ 21.13).
pub(crate) mod status {
    pub const SUCCESS: u16 = 0;
    pub const NO_ADDRS_AVAIL: u16 = 2;
    pub const NO_BINDING: u16 = 3;
    pub const NOT_ON_LINK: u16 = 4;
}

/// A message from a server: its type, transaction ID and options.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Message<'a> {
    pub kind: u8,
    pub xid: u32,
    options: &'a [u8],
}

impl<'a> Message<'a> {
    /// `None` if `data` is too short or an option runs past its end.
    pub fn parse(data: &'a [u8]) -> Option<Self> {
        let (&kind, rest) = data.split_first()?;
        let (xid, options) = rest.split_first_chunk::<3>()?;
        Options::check(options).then_some(Self {
            kind,
            xid: u32::from_be_bytes([0, xid[0], xid[1], xid[2]]),
            options,
        })
    }

    pub fn options(&self) -> Options<'a> {
        Options(self.options)
    }

    /// The first option with `code`.
    pub fn option(&self, code: u16) -> Option<&'a [u8]> {
        self.options().find(code)
    }

    /// The status of the message (§ 21.13); no Status Code option means
    /// success.
    pub fn status(&self) -> u16 {
        self.options().status()
    }
}

/// The options of a message or of an option, one after the other: `(code,
/// data)`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Options<'a>(&'a [u8]);

impl<'a> Options<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self(data)
    }

    /// Are the options in `data` all whole?
    pub fn check(mut data: &[u8]) -> bool {
        while !data.is_empty() {
            let Some((header, rest)) = data.split_first_chunk::<4>() else {
                return false;
            };
            let len = usize::from(u16::from_be_bytes([header[2], header[3]]));
            if rest.len() < len {
                return false;
            }
            data = &rest[len..];
        }
        true
    }

    pub fn find(self, code: u16) -> Option<&'a [u8]> {
        self.into_iter()
            .find_map(|(c, data)| (c == code).then_some(data))
    }

    pub fn status(self) -> u16 {
        self.find(opt::STATUS_CODE)
            .and_then(|data| data.first_chunk::<2>())
            .map_or(status::SUCCESS, |code| u16::from_be_bytes(*code))
    }
}

impl<'a> Iterator for Options<'a> {
    type Item = (u16, &'a [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        let (header, rest) = self.0.split_first_chunk::<4>()?;
        let code = u16::from_be_bytes([header[0], header[1]]);
        let len = usize::from(u16::from_be_bytes([header[2], header[3]]));
        // An option that doesn't fit ends the list.
        let Some((data, rest)) = rest.split_at_checked(len) else {
            self.0 = &[];
            return None;
        };
        self.0 = rest;
        Some((code, data))
    }
}

/// An IA_NA option (§ 21.4).
#[derive(Debug, Clone, Copy)]
pub(crate) struct IaNa<'a> {
    pub iaid: u32,
    pub t1: u32,
    pub t2: u32,
    pub options: Options<'a>,
}

impl<'a> IaNa<'a> {
    pub fn parse(data: &'a [u8]) -> Option<Self> {
        let (fixed, options) = data.split_first_chunk::<12>()?;
        Options::check(options).then_some(Self {
            iaid: u32::from_be_bytes([fixed[0], fixed[1], fixed[2], fixed[3]]),
            t1: u32::from_be_bytes([fixed[4], fixed[5], fixed[6], fixed[7]]),
            t2: u32::from_be_bytes([fixed[8], fixed[9], fixed[10], fixed[11]]),
            options: Options(options),
        })
    }

    /// The IA Address options in it.
    pub fn addresses(&self) -> impl Iterator<Item = IaAddress> + 'a {
        self.options
            .filter(|(code, _)| *code == opt::IA_ADDR)
            .filter_map(|(_, data)| IaAddress::parse(data))
    }
}

/// An IA Address option (§ 21.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IaAddress {
    pub address: Ipv6Addr,
    pub preferred: u32,
    pub valid: u32,
    pub status: u16,
}

impl IaAddress {
    fn parse(data: &[u8]) -> Option<Self> {
        let (address, rest) = data.split_first_chunk::<16>()?;
        let (preferred, rest) = rest.split_first_chunk::<4>()?;
        let (valid, options) = rest.split_first_chunk::<4>()?;
        if !Options::check(options) {
            return None;
        }
        Some(Self {
            address: Ipv6Addr::from(*address),
            preferred: u32::from_be_bytes(*preferred),
            valid: u32::from_be_bytes(*valid),
            status: Options(options).status(),
        })
    }
}

/// The 16-byte addresses in an option that is a list of them.
pub(crate) fn addresses(data: &[u8]) -> impl Iterator<Item = Ipv6Addr> + '_ {
    data.as_chunks::<16>().0.iter().copied().map(Ipv6Addr::from)
}

/// A 32-bit value option (SOL_MAX_RT, INF_MAX_RT, Information Refresh Time).
pub(crate) fn u32_option(data: &[u8]) -> Option<u32> {
    data.try_into().ok().map(u32::from_be_bytes)
}

/// The message did not fit the buffer [`crate::Client::poll`] was given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferTooSmall;

impl core::fmt::Display for BufferTooSmall {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("buffer too small for the DHCPv6 message")
    }
}

impl core::error::Error for BufferTooSmall {}

/// Writes a message into a buffer.
pub(crate) struct Writer<'a> {
    buf: &'a mut [u8],
    len: usize,
}

impl<'a> Writer<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, len: 0 }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn bytes(&mut self, bytes: &[u8]) -> Result<(), BufferTooSmall> {
        let end = self.len + bytes.len();
        self.buf
            .get_mut(self.len..end)
            .ok_or(BufferTooSmall)?
            .copy_from_slice(bytes);
        self.len = end;
        Ok(())
    }

    pub fn u16(&mut self, value: u16) -> Result<(), BufferTooSmall> {
        self.bytes(&value.to_be_bytes())
    }

    pub fn u32(&mut self, value: u32) -> Result<(), BufferTooSmall> {
        self.bytes(&value.to_be_bytes())
    }

    /// The message header: type and transaction ID.
    pub fn header(&mut self, kind: u8, xid: u32) -> Result<(), BufferTooSmall> {
        let xid = xid.to_be_bytes();
        self.bytes(&[kind, xid[1], xid[2], xid[3]])
    }

    /// An option whose data `body` writes.
    pub fn option(
        &mut self,
        code: u16,
        body: impl FnOnce(&mut Self) -> Result<(), BufferTooSmall>,
    ) -> Result<(), BufferTooSmall> {
        self.u16(code)?;
        let len_at = self.len;
        self.u16(0)?;
        body(self)?;
        let len = u16::try_from(self.len - len_at - 2).map_err(|_| BufferTooSmall)?;
        self.buf[len_at..len_at + 2].copy_from_slice(&len.to_be_bytes());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_message() {
        // Reply, xid 0x123456, Server ID 00:01, an empty option 99.
        let data = [7, 0x12, 0x34, 0x56, 0, 2, 0, 2, 0, 1, 0, 99, 0, 0];
        let message = Message::parse(&data).unwrap();
        assert_eq!(message.kind, msg::REPLY);
        assert_eq!(message.xid, 0x12_3456);
        assert_eq!(message.option(opt::SERVER_ID), Some([0, 1].as_slice()));
        assert_eq!(message.option(99), Some([].as_slice()));
        assert_eq!(message.option(opt::CLIENT_ID), None);
        assert_eq!(message.status(), status::SUCCESS);
    }

    #[test]
    fn truncated_messages_are_refused() {
        assert!(Message::parse(&[7, 0, 0]).is_none());
        assert!(Message::parse(&[7, 0, 0, 0]).is_some());
        // The option claims 3 bytes, 2 follow.
        assert!(Message::parse(&[7, 0, 0, 0, 0, 2, 0, 3, 0, 1]).is_none());
        // Half an option header.
        assert!(Message::parse(&[7, 0, 0, 0, 0, 2]).is_none());
    }

    #[test]
    fn status_code() {
        let data = [7, 0, 0, 1, 0, 13, 0, 4, 0, 2, b'n', b'o'];
        assert_eq!(
            Message::parse(&data).unwrap().status(),
            status::NO_ADDRS_AVAIL
        );
    }

    #[test]
    fn writer_fills_in_option_lengths() {
        let mut buf = [0u8; 32];
        let mut w = Writer::new(&mut buf);
        w.header(msg::SOLICIT, 0xAB_CDEF).unwrap();
        w.option(opt::IA_NA, |w| {
            w.u32(7)?;
            w.option(opt::STATUS_CODE, |w| w.u16(0))
        })
        .unwrap();
        let len = w.len();
        assert_eq!(
            &buf[..len],
            [1, 0xAB, 0xCD, 0xEF, 0, 3, 0, 10, 0, 0, 0, 7, 0, 13, 0, 2, 0, 0]
        );
    }

    #[test]
    fn writer_reports_a_full_buffer() {
        let mut buf = [0u8; 6];
        let mut w = Writer::new(&mut buf);
        w.header(msg::SOLICIT, 1).unwrap();
        assert_eq!(
            w.option(opt::ELAPSED_TIME, |w| w.u16(0)),
            Err(BufferTooSmall)
        );
    }

    #[test]
    fn ia_na_with_addresses() {
        let mut data = std::vec::Vec::new();
        data.extend_from_slice(&[0, 0, 0, 1, 0, 0, 0, 100, 0, 0, 0, 160]);
        // IA Address 2001:db8::5, 200/300 s, no status.
        data.extend_from_slice(&[0, 5, 0, 24]);
        data.extend_from_slice(&Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 5).octets());
        data.extend_from_slice(&[0, 0, 0, 200, 0, 0, 1, 44]);
        // A status for the IA_NA.
        data.extend_from_slice(&[0, 13, 0, 2, 0, 0]);
        let ia = IaNa::parse(&data).unwrap();
        assert_eq!((ia.iaid, ia.t1, ia.t2), (1, 100, 160));
        assert_eq!(ia.options.status(), status::SUCCESS);
        let addresses: std::vec::Vec<_> = ia.addresses().collect();
        assert_eq!(
            addresses,
            [IaAddress {
                address: Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 5),
                preferred: 200,
                valid: 300,
                status: status::SUCCESS,
            }]
        );
    }

    #[test]
    fn address_lists() {
        let mut data = std::vec::Vec::new();
        data.extend_from_slice(&Ipv6Addr::LOCALHOST.octets());
        data.extend_from_slice(&[1, 2, 3]);
        assert_eq!(
            addresses(&data).collect::<std::vec::Vec<_>>(),
            [Ipv6Addr::LOCALHOST]
        );
    }
}
