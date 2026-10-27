use bitflags::bitflags;
use byteorder::{ByteOrder, NetworkEndian};
use core::fmt;

use super::{Error, Result};
use crate::time::Duration;
use crate::wire::{Ipv6Address, Ipv6AddressExt, Ipv6Packet, Ipv6Repr, MAX_HARDWARE_ADDRESS_LEN};

use crate::wire::RawHardwareAddress;

enum_with_unknown! {
    /// NDISC Option Type
    pub enum Type(u8) {
        /// Source Link-layer Address
        SourceLinkLayerAddr = 0x1,
        /// Target Link-layer Address
        TargetLinkLayerAddr = 0x2,
        /// Prefix Information
        PrefixInformation   = 0x3,
        /// Redirected Header
        RedirectedHeader    = 0x4,
        /// MTU
        Mtu                 = 0x5,
        /// Recursive DNS Server
        RecursiveDnsServer  = 0x19
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Type::SourceLinkLayerAddr => write!(f, "source link-layer address"),
            Type::TargetLinkLayerAddr => write!(f, "target link-layer address"),
            Type::PrefixInformation => write!(f, "prefix information"),
            Type::RedirectedHeader => write!(f, "redirected header"),
            Type::Mtu => write!(f, "mtu"),
            Type::RecursiveDnsServer => write!(f, "recursive dns server"),
            Type::Unknown(id) => write!(f, "{id}"),
        }
    }
}

bitflags! {
    #[cfg_attr(feature = "defmt", derive(defmt::Format))]
    pub struct PrefixInfoFlags: u8 {
        const ON_LINK  = 0b10000000;
        const ADDRCONF = 0b01000000;
    }
}

/// A read/write wrapper around an [NDISC Option].
///
/// [NDISC Option]: https://tools.ietf.org/html/rfc4861#section-4.6
#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct NdiscOption<T: AsRef<[u8]>> {
    buffer: T,
}

// Format of an NDISC Option
//
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// |     Type      |    Length     |              ...              |
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
// ~                              ...                              ~
// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//
// See https://tools.ietf.org/html/rfc4861#section-4.6 for details.
mod field {
    #![allow(non_snake_case)]

    use crate::wire::field::*;

    // 8-bit identifier of the type of option.
    pub const TYPE: usize = 0;
    // 8-bit unsigned integer. Length of the option, in units of 8 octets.
    pub const LENGTH: usize = 1;
    // Minimum length of an option.
    pub const MIN_OPT_LEN: usize = 8;
    // Variable-length field. Option-Type-specific data.
    pub const fn DATA(length: u8) -> Field {
        2..length as usize * 8
    }

    // Source/Target Link-layer Option fields.
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    // |     Type      |    Length     |    Link-Layer Address ...
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+

    // Prefix Information Option fields.
    //  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    //  |     Type      |    Length     | Prefix Length |L|A| Reserved1 |
    //  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    //  |                         Valid Lifetime                        |
    //  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    //  |                       Preferred Lifetime                      |
    //  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    //  |                           Reserved2                           |
    //  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    //  |                                                               |
    //  +                                                               +
    //  |                                                               |
    //  +                            Prefix                             +
    //  |                                                               |
    //  +                                                               +
    //  |                                                               |
    //  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+

    // Prefix length.
    pub const PREFIX_LEN: usize = 2;
    // Flags field of prefix header.
    pub const FLAGS: usize = 3;
    // Valid lifetime.
    pub const VALID_LT: Field = 4..8;
    // Preferred lifetime.
    pub const PREF_LT: Field = 8..12;
    // Reserved bits
    pub const PREF_RESERVED: Field = 12..16;
    // Prefix
    pub const PREFIX: Field = 16..32;

    // Redirected Header Option fields.
    //  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    //  |     Type      |    Length     |            Reserved           |
    //  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    //  |                           Reserved                            |
    //  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    //  |                                                               |
    //  ~                       IP header + data                        ~
    //  |                                                               |
    //  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+

    // Reserved bits.
    pub const REDIRECTED_RESERVED: Field = 2..8;
    pub const REDIR_MIN_SZ: usize = 48;

    // MTU Option fields
    //  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    //  |     Type      |    Length     |           Reserved            |
    //  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    //  |                              MTU                              |
    //  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+

    //  MTU
    pub const MTU: Field = 4..8;

    // Recursive DNS Server Option fields (RFC 8106 § 5.1)
    //  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    //  |     Type      |     Length    |           Reserved            |
    //  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    //  |                           Lifetime                            |
    //  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    //  |                                                               |
    //  :            Addresses of IPv6 Recursive DNS Servers            :
    //  |                                                               |
    //  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+

    // Reserved bits.
    pub const RDNSS_RESERVED: Field = 2..4;
    // Lifetime.
    pub const RDNSS_LT: Field = 4..8;
    // Start of the addresses, 16 bytes each.
    pub const RDNSS_ADDRESSES: usize = 8;
}

/// Core getter methods relevant to any type of NDISC option.
impl<T: AsRef<[u8]>> NdiscOption<T> {
    /// Create a raw octet buffer with an NDISC Option structure.
    pub const fn new_unchecked(buffer: T) -> NdiscOption<T> {
        NdiscOption { buffer }
    }

    /// Shorthand for a combination of [new_unchecked] and [check_len].
    ///
    /// [new_unchecked]: #method.new_unchecked
    /// [check_len]: #method.check_len
    pub fn new_checked(buffer: T) -> Result<NdiscOption<T>> {
        let opt = Self::new_unchecked(buffer);
        opt.check_len()?;

        // A data length field of 0 is invalid.
        if opt.data_len() == 0 {
            return Err(Error);
        }

        Ok(opt)
    }

    /// Ensure that no accessor method will panic if called.
    /// Returns `Err(Error)` if the buffer is too short.
    ///
    /// The result of this check is invalidated by calling [set_data_len].
    ///
    /// [set_data_len]: #method.set_data_len
    pub fn check_len(&self) -> Result<()> {
        let data = self.buffer.as_ref();
        let len = data.len();

        if len < field::MIN_OPT_LEN {
            Err(Error)
        } else {
            let data_range = field::DATA(data[field::LENGTH]);
            if len < data_range.end {
                Err(Error)
            } else {
                match self.option_type() {
                    Type::SourceLinkLayerAddr | Type::TargetLinkLayerAddr | Type::Mtu => Ok(()),
                    Type::PrefixInformation if data_range.end >= field::PREFIX.end => Ok(()),
                    Type::RedirectedHeader if data_range.end >= field::REDIR_MIN_SZ => Ok(()),
                    // At least one address.
                    Type::RecursiveDnsServer if data_range.end >= field::RDNSS_ADDRESSES + 16 => {
                        Ok(())
                    }
                    Type::Unknown(_) => Ok(()),
                    _ => Err(Error),
                }
            }
        }
    }

    /// Consume the NDISC option, returning the underlying buffer.
    pub fn into_inner(self) -> T {
        self.buffer
    }

    /// Return the option type.
    #[inline]
    pub fn option_type(&self) -> Type {
        let data = self.buffer.as_ref();
        Type::from(data[field::TYPE])
    }

    /// Return the length of the data.
    #[inline]
    pub fn data_len(&self) -> u8 {
        let data = self.buffer.as_ref();
        data[field::LENGTH]
    }
}

/// Getter methods only relevant for Source/Target Link-layer Address options.
impl<T: AsRef<[u8]>> NdiscOption<T> {
    /// Return the Source/Target Link-layer Address.
    #[inline]
    pub fn link_layer_addr(&self) -> RawHardwareAddress {
        let len = MAX_HARDWARE_ADDRESS_LEN.min(self.data_len() as usize * 8 - 2);
        let data = self.buffer.as_ref();
        RawHardwareAddress::from_bytes(&data[2..len + 2])
    }
}

/// Getter methods only relevant for the MTU option.
impl<T: AsRef<[u8]>> NdiscOption<T> {
    /// Return the MTU value.
    #[inline]
    pub fn mtu(&self) -> u32 {
        let data = self.buffer.as_ref();
        NetworkEndian::read_u32(&data[field::MTU])
    }
}

/// Getter methods only relevant for the Recursive DNS Server option.
impl<T: AsRef<[u8]>> NdiscOption<T> {
    /// Return the lifetime of the servers.
    #[inline]
    pub fn rdnss_lifetime(&self) -> Duration {
        let data = self.buffer.as_ref();
        Duration::from_secs(NetworkEndian::read_u32(&data[field::RDNSS_LT]) as u64)
    }
}

/// Getter methods only relevant for the Prefix Information option.
impl<T: AsRef<[u8]>> NdiscOption<T> {
    /// Return the prefix length.
    #[inline]
    pub fn prefix_len(&self) -> u8 {
        self.buffer.as_ref()[field::PREFIX_LEN]
    }

    /// Return the prefix information flags.
    #[inline]
    pub fn prefix_flags(&self) -> PrefixInfoFlags {
        PrefixInfoFlags::from_bits_truncate(self.buffer.as_ref()[field::FLAGS])
    }

    /// Return the valid lifetime of the prefix.
    #[inline]
    pub fn valid_lifetime(&self) -> Duration {
        let data = self.buffer.as_ref();
        Duration::from_secs(NetworkEndian::read_u32(&data[field::VALID_LT]) as u64)
    }

    /// Return the preferred lifetime of the prefix.
    #[inline]
    pub fn preferred_lifetime(&self) -> Duration {
        let data = self.buffer.as_ref();
        Duration::from_secs(NetworkEndian::read_u32(&data[field::PREF_LT]) as u64)
    }

    /// Return the prefix.
    #[inline]
    pub fn prefix(&self) -> Ipv6Address {
        let data = self.buffer.as_ref();
        Ipv6Address::from_octets(data[field::PREFIX].try_into().unwrap())
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> NdiscOption<&'a T> {
    /// Return the option data.
    #[inline]
    pub fn data(&self) -> &'a [u8] {
        let len = self.data_len();
        let data = self.buffer.as_ref();
        &data[field::DATA(len)]
    }
}

/// Core setter methods relevant to any type of NDISC option.
impl<T: AsRef<[u8]> + AsMut<[u8]>> NdiscOption<T> {
    /// Set the option type.
    #[inline]
    pub fn set_option_type(&mut self, value: Type) {
        let data = self.buffer.as_mut();
        data[field::TYPE] = value.into();
    }

    /// Set the option data length.
    #[inline]
    pub fn set_data_len(&mut self, value: u8) {
        let data = self.buffer.as_mut();
        data[field::LENGTH] = value;
    }
}

/// Setter methods only relevant for Source/Target Link-layer Address options.
impl<T: AsRef<[u8]> + AsMut<[u8]>> NdiscOption<T> {
    /// Set the Source/Target Link-layer Address.
    #[inline]
    pub fn set_link_layer_addr(&mut self, addr: RawHardwareAddress) {
        let data = self.buffer.as_mut();
        data[2..2 + addr.len()].copy_from_slice(addr.as_bytes())
    }
}

/// Setter methods only relevant for the MTU option.
impl<T: AsRef<[u8]> + AsMut<[u8]>> NdiscOption<T> {
    /// Set the MTU value.
    #[inline]
    pub fn set_mtu(&mut self, value: u32) {
        let data = self.buffer.as_mut();
        NetworkEndian::write_u32(&mut data[field::MTU], value);
    }
}

/// Setter methods only relevant for the Recursive DNS Server option.
impl<T: AsRef<[u8]> + AsMut<[u8]>> NdiscOption<T> {
    /// Clear the reserved bits.
    #[inline]
    pub fn clear_rdnss_reserved(&mut self) {
        let data = self.buffer.as_mut();
        data[field::RDNSS_RESERVED].fill(0);
    }

    /// Set the lifetime of the servers.
    #[inline]
    pub fn set_rdnss_lifetime(&mut self, value: Duration) {
        let data = self.buffer.as_mut();
        NetworkEndian::write_u32(&mut data[field::RDNSS_LT], value.secs() as u32);
    }
}

/// Setter methods only relevant for the Prefix Information option.
impl<T: AsRef<[u8]> + AsMut<[u8]>> NdiscOption<T> {
    /// Set the prefix length.
    #[inline]
    pub fn set_prefix_len(&mut self, value: u8) {
        self.buffer.as_mut()[field::PREFIX_LEN] = value;
    }

    /// Set the prefix information flags.
    #[inline]
    pub fn set_prefix_flags(&mut self, flags: PrefixInfoFlags) {
        self.buffer.as_mut()[field::FLAGS] = flags.bits();
    }

    /// Set the valid lifetime of the prefix.
    #[inline]
    pub fn set_valid_lifetime(&mut self, time: Duration) {
        let data = self.buffer.as_mut();
        NetworkEndian::write_u32(&mut data[field::VALID_LT], time.secs() as u32);
    }

    /// Set the preferred lifetime of the prefix.
    #[inline]
    pub fn set_preferred_lifetime(&mut self, time: Duration) {
        let data = self.buffer.as_mut();
        NetworkEndian::write_u32(&mut data[field::PREF_LT], time.secs() as u32);
    }

    /// Clear the reserved bits.
    #[inline]
    pub fn clear_prefix_reserved(&mut self) {
        let data = self.buffer.as_mut();
        NetworkEndian::write_u32(&mut data[field::PREF_RESERVED], 0);
    }

    /// Set the prefix.
    #[inline]
    pub fn set_prefix(&mut self, addr: Ipv6Address) {
        let data = self.buffer.as_mut();
        data[field::PREFIX].copy_from_slice(&addr.octets());
    }
}

/// Setter methods only relevant for the Redirected Header option.
impl<T: AsRef<[u8]> + AsMut<[u8]>> NdiscOption<T> {
    /// Clear the reserved bits.
    #[inline]
    pub fn clear_redirected_reserved(&mut self) {
        let data = self.buffer.as_mut();
        data[field::REDIRECTED_RESERVED].fill_with(|| 0);
    }
}

impl<T: AsRef<[u8]> + AsMut<[u8]> + ?Sized> NdiscOption<&mut T> {
    /// Return a mutable pointer to the option data.
    #[inline]
    pub fn data_mut(&mut self) -> &mut [u8] {
        let len = self.data_len();
        let data = self.buffer.as_mut();
        &mut data[field::DATA(len)]
    }
}

impl<T: AsRef<[u8]> + ?Sized> fmt::Display for NdiscOption<&T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match Repr::parse(self) {
            Ok(repr) => write!(f, "{repr}"),
            Err(err) => {
                write!(f, "NDISC Option ({err})")?;
                Ok(())
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct PrefixInformation {
    pub prefix_len: u8,
    pub flags: PrefixInfoFlags,
    pub valid_lifetime: Duration,
    pub preferred_lifetime: Duration,
    pub prefix: Ipv6Address,
}

impl PrefixInformation {
    /// Validates the prefix information option against check a, b, c in
    /// <https://www.rfc-editor.org/rfc/rfc4862#section-5.5.3>
    pub fn is_valid_prefix_info(&self) -> bool {
        self.flags.contains(PrefixInfoFlags::ADDRCONF)
            && !self.prefix.is_link_local()
            && self.preferred_lifetime <= self.valid_lifetime
    }
}

/// The DNS servers of a Recursive DNS Server option ([RFC 8106 § 5.1]).
///
/// [RFC 8106 § 5.1]: https://www.rfc-editor.org/rfc/rfc8106#section-5.1
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct RecursiveDnsServer<'a> {
    /// How long the servers may be used; zero means no longer, all ones
    /// (`0xffffffff` seconds) means forever.
    pub lifetime: Duration,
    /// The servers' addresses, 16 bytes each.
    pub addresses: &'a [u8],
}

impl<'a> RecursiveDnsServer<'a> {
    /// The servers' addresses.
    pub fn addresses(&self) -> impl Iterator<Item = Ipv6Address> + 'a {
        let (addresses, _) = self.addresses.as_chunks::<16>();
        addresses
            .iter()
            .map(|octets| Ipv6Address::from_octets(*octets))
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct RedirectedHeader<'a> {
    pub header: Ipv6Repr,
    pub data: &'a [u8],
}

/// A high-level representation of an NDISC Option.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Repr<'a> {
    SourceLinkLayerAddr(RawHardwareAddress),
    TargetLinkLayerAddr(RawHardwareAddress),
    PrefixInformation(PrefixInformation),
    RedirectedHeader(RedirectedHeader<'a>),
    Mtu(u32),
    RecursiveDnsServer(RecursiveDnsServer<'a>),
    Unknown {
        type_: u8,
        length: u8,
        data: &'a [u8],
    },
}

impl<'a> Repr<'a> {
    /// Parse an NDISC Option and return a high-level representation.
    pub fn parse<T>(opt: &NdiscOption<&'a T>) -> Result<Repr<'a>>
    where
        T: AsRef<[u8]> + ?Sized,
    {
        opt.check_len()?;

        match opt.option_type() {
            Type::SourceLinkLayerAddr => {
                if opt.data_len() >= 1 {
                    Ok(Repr::SourceLinkLayerAddr(opt.link_layer_addr()))
                } else {
                    Err(Error)
                }
            }
            Type::TargetLinkLayerAddr => {
                if opt.data_len() >= 1 {
                    Ok(Repr::TargetLinkLayerAddr(opt.link_layer_addr()))
                } else {
                    Err(Error)
                }
            }
            Type::PrefixInformation => {
                if opt.data_len() == 4 {
                    Ok(Repr::PrefixInformation(PrefixInformation {
                        prefix_len: opt.prefix_len(),
                        flags: opt.prefix_flags(),
                        valid_lifetime: opt.valid_lifetime(),
                        preferred_lifetime: opt.preferred_lifetime(),
                        prefix: opt.prefix(),
                    }))
                } else {
                    Err(Error)
                }
            }
            Type::RedirectedHeader => {
                // If the options data length is less than 6, the option
                // does not have enough data to fill out the IP header
                // and common option fields.
                if opt.data_len() < 6 {
                    Err(Error)
                } else {
                    let redirected_packet = &opt.data()[field::REDIRECTED_RESERVED.len()..];

                    let ip_packet = Ipv6Packet::new_checked(redirected_packet)?;
                    let ip_repr = Ipv6Repr::parse(&ip_packet)?;

                    Ok(Repr::RedirectedHeader(RedirectedHeader {
                        header: ip_repr,
                        data: &redirected_packet[ip_repr.buffer_len()..][..ip_repr.payload_len],
                    }))
                }
            }
            Type::Mtu => {
                if opt.data_len() == 1 {
                    Ok(Repr::Mtu(opt.mtu()))
                } else {
                    Err(Error)
                }
            }
            Type::RecursiveDnsServer => {
                // The 8 bytes of the header, then whole addresses: an odd
                // length (RFC 8106 § 5.3.1).
                if opt.data_len() >= 3 && opt.data_len() % 2 == 1 {
                    Ok(Repr::RecursiveDnsServer(RecursiveDnsServer {
                        lifetime: opt.rdnss_lifetime(),
                        addresses: &opt.data()[field::RDNSS_ADDRESSES - 2..],
                    }))
                } else {
                    Err(Error)
                }
            }
            Type::Unknown(id) => {
                // A length of 0 is invalid.
                if opt.data_len() != 0 {
                    Ok(Repr::Unknown {
                        type_: id,
                        length: opt.data_len(),
                        data: opt.data(),
                    })
                } else {
                    Err(Error)
                }
            }
        }
    }

    /// Return the length of a header that will be emitted from this high-level representation.
    pub const fn buffer_len(&self) -> usize {
        match self {
            &Repr::SourceLinkLayerAddr(addr) | &Repr::TargetLinkLayerAddr(addr) => {
                let len = 2 + addr.len();
                // Round up to next multiple of 8
                len.div_ceil(8) * 8
            }
            &Repr::PrefixInformation(_) => field::PREFIX.end,
            &Repr::RedirectedHeader(RedirectedHeader { header, data }) => {
                (8 + header.buffer_len() + data.len()).div_ceil(8) * 8
            }
            &Repr::Mtu(_) => field::MTU.end,
            &Repr::RecursiveDnsServer(RecursiveDnsServer { addresses, .. }) => {
                field::RDNSS_ADDRESSES + addresses.len()
            }
            &Repr::Unknown { length, .. } => field::DATA(length).end,
        }
    }

    /// Emit a high-level representation into an NDISC Option.
    pub fn emit<T>(&self, opt: &mut NdiscOption<&'a mut T>)
    where
        T: AsRef<[u8]> + AsMut<[u8]> + ?Sized,
    {
        match *self {
            Repr::SourceLinkLayerAddr(addr) => {
                opt.set_option_type(Type::SourceLinkLayerAddr);
                let opt_len = addr.len() + 2;
                opt.set_data_len(opt_len.div_ceil(8) as u8); // round to next multiple of 8.
                opt.set_link_layer_addr(addr);
            }
            Repr::TargetLinkLayerAddr(addr) => {
                opt.set_option_type(Type::TargetLinkLayerAddr);
                let opt_len = addr.len() + 2;
                opt.set_data_len(opt_len.div_ceil(8) as u8); // round to next multiple of 8.
                opt.set_link_layer_addr(addr);
            }
            Repr::PrefixInformation(PrefixInformation {
                prefix_len,
                flags,
                valid_lifetime,
                preferred_lifetime,
                prefix,
            }) => {
                opt.clear_prefix_reserved();
                opt.set_option_type(Type::PrefixInformation);
                opt.set_data_len(4);
                opt.set_prefix_len(prefix_len);
                opt.set_prefix_flags(flags);
                opt.set_valid_lifetime(valid_lifetime);
                opt.set_preferred_lifetime(preferred_lifetime);
                opt.set_prefix(prefix);
            }
            Repr::RedirectedHeader(RedirectedHeader { header, data }) => {
                // TODO(thvdveld): I think we need to check if the data we are sending is not
                // exceeding the MTU.
                opt.clear_redirected_reserved();
                opt.set_option_type(Type::RedirectedHeader);
                opt.set_data_len((8 + header.buffer_len() + data.len()).div_ceil(8) as u8);
                let mut packet = &mut opt.data_mut()[field::REDIRECTED_RESERVED.end - 2..];
                let mut ip_packet = Ipv6Packet::new_unchecked(&mut packet);
                header.emit(&mut ip_packet);
                ip_packet.payload_mut().copy_from_slice(data);
            }
            Repr::Mtu(mtu) => {
                opt.set_option_type(Type::Mtu);
                opt.set_data_len(1);
                opt.set_mtu(mtu);
            }
            Repr::RecursiveDnsServer(RecursiveDnsServer {
                lifetime,
                addresses,
            }) => {
                opt.set_option_type(Type::RecursiveDnsServer);
                opt.set_data_len(((field::RDNSS_ADDRESSES + addresses.len()) / 8) as u8);
                opt.clear_rdnss_reserved();
                opt.set_rdnss_lifetime(lifetime);
                opt.data_mut()[field::RDNSS_ADDRESSES - 2..].copy_from_slice(addresses);
            }
            Repr::Unknown {
                type_: id,
                length,
                data,
            } => {
                opt.set_option_type(Type::Unknown(id));
                opt.set_data_len(length);
                opt.data_mut().copy_from_slice(data);
            }
        }
    }
}

impl<'a> fmt::Display for Repr<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "NDISC Option: ")?;
        match *self {
            Repr::SourceLinkLayerAddr(addr) => {
                write!(f, "SourceLinkLayer addr={addr}")
            }
            Repr::TargetLinkLayerAddr(addr) => {
                write!(f, "TargetLinkLayer addr={addr}")
            }
            Repr::PrefixInformation(PrefixInformation {
                prefix, prefix_len, ..
            }) => {
                write!(f, "PrefixInformation prefix={prefix}/{prefix_len}")
            }
            Repr::RedirectedHeader(RedirectedHeader { header, .. }) => {
                write!(f, "RedirectedHeader header={header}")
            }
            Repr::Mtu(mtu) => {
                write!(f, "MTU mtu={mtu}")
            }
            Repr::RecursiveDnsServer(RecursiveDnsServer {
                lifetime,
                addresses,
            }) => {
                write!(
                    f,
                    "RecursiveDnsServer lifetime={lifetime} servers={}",
                    addresses.len() / 16
                )
            }
            Repr::Unknown {
                type_: id, length, ..
            } => {
                write!(f, "Unknown({id}) length={length}")
            }
        }
    }
}

/// An iterator over the options of a Neighbor Discovery message, such as
/// the several Prefix Information options a Router Advertisement can carry.
///
/// Options that don't parse are skipped, like [`super::NdiscRepr::parse`]
/// does. Iteration ends at the first option whose length is zero or longer
/// than the remaining data.
#[derive(Debug, Clone)]
pub struct Iter<'a> {
    data: &'a [u8],
}

impl<'a> Iter<'a> {
    /// Iterates over the options in `data`, the payload of an NDISC message
    /// (`Icmpv6Packet::payload`).
    pub fn new(data: &'a [u8]) -> Self {
        Self { data }
    }
}

impl<'a> Iterator for Iter<'a> {
    type Item = Repr<'a>;

    fn next(&mut self) -> Option<Repr<'a>> {
        loop {
            if self.data.len() < field::MIN_OPT_LEN {
                return None;
            }
            let len = self.data[field::LENGTH] as usize * 8;
            if len == 0 || len > self.data.len() {
                self.data = &[];
                return None;
            }
            let (option, rest) = self.data.split_at(len);
            self.data = rest;
            if let Ok(repr) = NdiscOption::new_checked(option).and_then(|o| Repr::parse(&o)) {
                return Some(repr);
            }
        }
    }
}

use crate::wire::pretty_print::{PrettyIndent, PrettyPrint};

impl<T: AsRef<[u8]>> PrettyPrint for NdiscOption<T> {
    fn pretty_print(
        buffer: &dyn AsRef<[u8]>,
        f: &mut fmt::Formatter,
        indent: &mut PrettyIndent,
    ) -> fmt::Result {
        match NdiscOption::new_checked(buffer) {
            Err(err) => write!(f, "{indent}({err})"),
            Ok(ndisc) => match Repr::parse(&ndisc) {
                Err(_) => Ok(()),
                Ok(repr) => {
                    write!(f, "{indent}{repr}")
                }
            },
        }
    }
}

#[cfg(any(feature = "medium-ethernet", feature = "medium-ieee802154"))]
#[cfg(test)]
mod test {
    use super::Error;
    use super::{NdiscOption, PrefixInfoFlags, PrefixInformation, Repr, Type};
    use crate::time::Duration;
    use crate::wire::Ipv6Address;

    #[cfg(feature = "medium-ethernet")]
    use crate::wire::EthernetAddress;
    #[cfg(all(not(feature = "medium-ethernet"), feature = "medium-ieee802154"))]
    use crate::wire::Ieee802154Address;

    static PREFIX_OPT_BYTES: [u8; 32] = [
        0x03, 0x04, 0x40, 0xc0, 0x00, 0x00, 0x03, 0x84, 0x00, 0x00, 0x03, 0xe8, 0x00, 0x00, 0x00,
        0x00, 0xfe, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x01,
    ];

    #[test]
    fn test_deconstruct() {
        let opt = NdiscOption::new_unchecked(&PREFIX_OPT_BYTES[..]);
        assert_eq!(opt.option_type(), Type::PrefixInformation);
        assert_eq!(opt.data_len(), 4);
        assert_eq!(opt.prefix_len(), 64);
        assert_eq!(
            opt.prefix_flags(),
            PrefixInfoFlags::ON_LINK | PrefixInfoFlags::ADDRCONF
        );
        assert_eq!(opt.valid_lifetime(), Duration::from_secs(900));
        assert_eq!(opt.preferred_lifetime(), Duration::from_secs(1000));
        assert_eq!(opt.prefix(), Ipv6Address::new(0xfe80, 0, 0, 0, 0, 0, 0, 1));
    }

    #[test]
    fn test_construct() {
        let mut bytes = [0x00; 32];
        let mut opt = NdiscOption::new_unchecked(&mut bytes[..]);
        opt.set_option_type(Type::PrefixInformation);
        opt.set_data_len(4);
        opt.set_prefix_len(64);
        opt.set_prefix_flags(PrefixInfoFlags::ON_LINK | PrefixInfoFlags::ADDRCONF);
        opt.set_valid_lifetime(Duration::from_secs(900));
        opt.set_preferred_lifetime(Duration::from_secs(1000));
        opt.set_prefix(Ipv6Address::new(0xfe80, 0, 0, 0, 0, 0, 0, 1));
        assert_eq!(&PREFIX_OPT_BYTES[..], &*opt.into_inner());
    }

    #[test]
    fn test_short_packet() {
        assert_eq!(NdiscOption::new_checked(&[0x00, 0x00]), Err(Error));
        let bytes = [0x03, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(NdiscOption::new_checked(&bytes), Err(Error));
    }

    #[cfg(feature = "medium-ethernet")]
    #[test]
    fn test_repr_parse_link_layer_opt_ethernet() {
        let mut bytes = [0x01, 0x01, 0x54, 0x52, 0x00, 0x12, 0x23, 0x34];
        let addr = EthernetAddress([0x54, 0x52, 0x00, 0x12, 0x23, 0x34]);
        {
            assert_eq!(
                Repr::parse(&NdiscOption::new_unchecked(&bytes)),
                Ok(Repr::SourceLinkLayerAddr(addr.into()))
            );
        }
        bytes[0] = 0x02;
        {
            assert_eq!(
                Repr::parse(&NdiscOption::new_unchecked(&bytes)),
                Ok(Repr::TargetLinkLayerAddr(addr.into()))
            );
        }
    }

    #[cfg(all(not(feature = "medium-ethernet"), feature = "medium-ieee802154"))]
    #[test]
    fn test_repr_parse_link_layer_opt_ieee802154() {
        let mut bytes = [
            0x01, 0x02, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00,
        ];
        let addr = Ieee802154Address::Extended([0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);
        {
            assert_eq!(
                Repr::parse(&NdiscOption::new_unchecked(&bytes)),
                Ok(Repr::SourceLinkLayerAddr(addr.into()))
            );
        }
        bytes[0] = 0x02;
        {
            assert_eq!(
                Repr::parse(&NdiscOption::new_unchecked(&bytes)),
                Ok(Repr::TargetLinkLayerAddr(addr.into()))
            );
        }
    }

    #[test]
    fn test_repr_parse_prefix_info() {
        let repr = Repr::PrefixInformation(PrefixInformation {
            prefix_len: 64,
            flags: PrefixInfoFlags::ON_LINK | PrefixInfoFlags::ADDRCONF,
            valid_lifetime: Duration::from_secs(900),
            preferred_lifetime: Duration::from_secs(1000),
            prefix: Ipv6Address::new(0xfe80, 0, 0, 0, 0, 0, 0, 1),
        });
        assert_eq!(
            Repr::parse(&NdiscOption::new_unchecked(&PREFIX_OPT_BYTES)),
            Ok(repr)
        );
    }

    #[test]
    fn test_repr_emit_prefix_info() {
        let mut bytes = [0x2a; 32];
        let repr = Repr::PrefixInformation(PrefixInformation {
            prefix_len: 64,
            flags: PrefixInfoFlags::ON_LINK | PrefixInfoFlags::ADDRCONF,
            valid_lifetime: Duration::from_secs(900),
            preferred_lifetime: Duration::from_secs(1000),
            prefix: Ipv6Address::new(0xfe80, 0, 0, 0, 0, 0, 0, 1),
        });
        let mut opt = NdiscOption::new_unchecked(&mut bytes);
        repr.emit(&mut opt);
        assert_eq!(&opt.into_inner()[..], &PREFIX_OPT_BYTES[..]);
    }

    #[test]
    fn test_repr_parse_mtu() {
        let bytes = [0x05, 0x01, 0x00, 0x00, 0x00, 0x00, 0x05, 0xdc];
        assert_eq!(
            Repr::parse(&NdiscOption::new_unchecked(&bytes)),
            Ok(Repr::Mtu(1500))
        );
    }

    // Lifetime 600 s, servers 2001:db8::53 and 2001:db8::54.
    static RDNSS_OPT_BYTES: [u8; 40] = [
        0x19, 0x05, 0x00, 0x00, 0x00, 0x00, 0x02, 0x58, 0x20, 0x01, 0x0d, 0xb8, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x53, 0x20, 0x01, 0x0d, 0xb8, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x54,
    ];

    #[test]
    fn test_repr_parse_rdnss() {
        let Ok(Repr::RecursiveDnsServer(rdnss)) =
            Repr::parse(&NdiscOption::new_unchecked(&RDNSS_OPT_BYTES[..]))
        else {
            panic!("not parsed as RDNSS");
        };
        assert_eq!(rdnss.lifetime, Duration::from_secs(600));
        assert_eq!(
            rdnss.addresses().collect::<std::vec::Vec<_>>(),
            [
                Ipv6Address::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 0x53),
                Ipv6Address::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 0x54)
            ]
        );
    }

    #[test]
    fn test_repr_emit_rdnss() {
        let repr = Repr::RecursiveDnsServer(super::RecursiveDnsServer {
            lifetime: Duration::from_secs(600),
            addresses: &RDNSS_OPT_BYTES[8..],
        });
        assert_eq!(repr.buffer_len(), RDNSS_OPT_BYTES.len());
        let mut bytes = [0xff; 40];
        repr.emit(&mut NdiscOption::new_unchecked(&mut bytes[..]));
        assert_eq!(bytes, RDNSS_OPT_BYTES);
    }

    #[test]
    fn test_rdnss_needs_whole_addresses() {
        // Length 1: no address. Length 4: an address and a half.
        let mut none = [0u8; 8];
        none[..2].copy_from_slice(&[0x19, 0x01]);
        assert_eq!(NdiscOption::new_checked(&none[..]), Err(Error));
        let mut half = [0u8; 32];
        half[..2].copy_from_slice(&[0x19, 0x04]);
        assert_eq!(
            Repr::parse(&NdiscOption::new_checked(&half[..]).unwrap()),
            Err(Error)
        );
    }

    #[test]
    fn test_iter_yields_every_option() {
        let mut second = PREFIX_OPT_BYTES;
        second[31] = 0x02;
        let mtu = [0x05, 0x01, 0x00, 0x00, 0x00, 0x00, 0x05, 0xdc];
        // A Prefix Information option one block too short doesn't parse.
        let short = [0x03, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let mut bytes = std::vec::Vec::new();
        for option in [&PREFIX_OPT_BYTES[..], &short, &mtu, &second] {
            bytes.extend_from_slice(option);
        }

        let prefixes: std::vec::Vec<_> = super::Iter::new(&bytes)
            .filter_map(|repr| match repr {
                Repr::PrefixInformation(info) => Some(info.prefix),
                _ => None,
            })
            .collect();
        assert_eq!(
            prefixes,
            [
                Ipv6Address::new(0xfe80, 0, 0, 0, 0, 0, 0, 1),
                Ipv6Address::new(0xfe80, 0, 0, 0, 0, 0, 0, 2)
            ]
        );
        assert_eq!(super::Iter::new(&bytes).count(), 3);
    }

    #[test]
    fn test_iter_stops_at_a_bad_length() {
        let mtu = [0x05, 0x01, 0x00, 0x00, 0x00, 0x00, 0x05, 0xdc];
        let zero = [0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0xdc];
        let long = [0x05, 0x03, 0x00, 0x00, 0x00, 0x00, 0x05, 0xdc];
        for bad in [zero, long] {
            let bytes = [mtu, bad, mtu].concat();
            assert_eq!(super::Iter::new(&bytes).count(), 1);
        }
        // Trailing bytes too few for an option are ignored.
        assert_eq!(super::Iter::new(&[0x05, 0x01, 0x00]).count(), 0);
    }
}
