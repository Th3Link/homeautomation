//! Domain names as DHCPv6 carries them (RFC 8415 § 10): the DNS wire format
//! of RFC 1035 § 3.1, without compression.

use core::fmt;
use heapless::Vec;

/// The longest name on the wire (RFC 1035 § 2.3.4).
const NAME_MAX: usize = 255;
const LABEL_MAX: u8 = 63;

/// A list of domain names in at most `N` bytes, kept in their wire format.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Names<const N: usize>(Vec<u8, N>);

impl<const N: usize> Names<N> {
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    pub fn iter(&self) -> impl Iterator<Item = Name<'_>> {
        let mut rest = self.0.as_slice();
        core::iter::from_fn(move || {
            // Only whole, valid names are ever stored.
            let len = name_len(rest)?;
            let (name, tail) = rest.split_at(len);
            rest = tail;
            Some(Name(name))
        })
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn clear(&mut self) {
        self.0.clear();
    }

    /// Adds the valid names of `list` (names one after the other, as in the
    /// Domain Search List option) that still fit; stops at the first
    /// malformed one. A name already in the list is not added again.
    pub(crate) fn extend_from_wire(&mut self, mut list: &[u8]) {
        while let Some(len) = name_len(list) {
            let (name, rest) = list.split_at(len);
            self.push_wire(name);
            list = rest;
        }
    }

    /// Adds one name in wire format, if it is valid, new and fits.
    pub(crate) fn push_wire(&mut self, name: &[u8]) {
        if name_len(name) != Some(name.len()) || self.iter().any(|known| known.0 == name) {
            return;
        }
        let _ = self.0.extend_from_slice(name);
    }
}

/// The length of the name at the start of `data`, including the final
/// zero byte; `None` if it isn't a valid uncompressed name.
fn name_len(data: &[u8]) -> Option<usize> {
    let mut at = 0;
    loop {
        let label = *data.get(at)?;
        // Also rejects compression pointers (0b11xx_xxxx).
        if label > LABEL_MAX {
            return None;
        }
        at += 1 + usize::from(label);
        if at > NAME_MAX {
            return None;
        }
        if label == 0 {
            return Some(at);
        }
    }
}

/// One domain name in wire format; shown as dotted text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Name<'a>(&'a [u8]);

impl<'a> Name<'a> {
    /// The name's labels.
    pub fn labels(&self) -> impl Iterator<Item = &'a [u8]> {
        let mut rest = self.0;
        core::iter::from_fn(move || {
            let (&len, tail) = rest.split_first()?;
            if len == 0 {
                return None;
            }
            let (label, tail) = tail.split_at(usize::from(len));
            rest = tail;
            Some(label)
        })
    }

    pub fn as_wire(&self) -> &'a [u8] {
        self.0
    }
}

/// `example.com`; the root is `.`. Bytes outside printable ASCII, and the
/// dot inside a label, are written `\DDD` (RFC 4343).
impl fmt::Display for Name<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut any = false;
        for label in self.labels() {
            if any {
                f.write_str(".")?;
            }
            any = true;
            for &byte in label {
                if byte.is_ascii_graphic() && byte != b'.' && byte != b'\\' {
                    write!(f, "{}", char::from(byte))?;
                } else {
                    write!(f, "\\{byte:03}")?;
                }
            }
        }
        if !any {
            f.write_str(".")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::string::{String, ToString};
    use std::vec::Vec as StdVec;

    fn names<const N: usize>(list: &Names<N>) -> StdVec<String> {
        list.iter().map(|name| name.to_string()).collect()
    }

    const EXAMPLE: &[u8] = b"\x07example\x03com\x00";
    const LAN: &[u8] = b"\x03lan\x00";

    #[test]
    fn list_of_names() {
        let mut list = Names::<64>::new();
        list.extend_from_wire(&[EXAMPLE, LAN].concat());
        assert_eq!(names(&list), ["example.com", "lan"]);
    }

    #[test]
    fn duplicates_are_left_out() {
        let mut list = Names::<64>::new();
        list.extend_from_wire(&[EXAMPLE, LAN, EXAMPLE].concat());
        list.push_wire(LAN);
        assert_eq!(names(&list), ["example.com", "lan"]);
    }

    #[test]
    fn what_does_not_fit_is_left_out() {
        let mut list = Names::<17>::new();
        list.extend_from_wire(&[EXAMPLE, LAN, LAN].concat());
        list.push_wire(b"\x02ab\x00");
        // example.com (13 bytes) fits, lan (5) no longer, ab (4) still does.
        assert_eq!(names(&list), ["example.com", "ab"]);
    }

    #[test]
    fn malformed_names_end_the_list() {
        let mut list = Names::<64>::new();
        // A compression pointer, then a name.
        list.extend_from_wire(&[b"\xc0\x0c".as_slice(), LAN].concat());
        assert!(list.is_empty());
        // A name without its final zero byte.
        list.extend_from_wire(&[LAN, b"\x03lan".as_slice()].concat());
        assert_eq!(names(&list), ["lan"]);
        // A label longer than 63 bytes.
        let mut long = vec![64u8];
        long.extend_from_slice(&[b'a'; 64]);
        long.push(0);
        list.push_wire(&long);
        assert_eq!(names(&list), ["lan"]);
    }

    #[test]
    fn names_longer_than_255_bytes_are_refused() {
        let mut name = StdVec::new();
        for _ in 0..4 {
            name.push(63);
            name.extend_from_slice(&[b'a'; 63]);
        }
        name.push(0);
        assert_eq!(name.len(), 257);
        assert_eq!(name_len(&name), None);
        name.truncate(192);
        name.push(61);
        name.extend_from_slice(&[b'a'; 61]);
        name.push(0);
        assert_eq!(name_len(&name), Some(255));
    }

    #[test]
    fn display_escapes() {
        let mut list = Names::<64>::new();
        list.push_wire(b"\x03a.b\x02c\x01\x00");
        list.push_wire(b"\x00");
        assert_eq!(names(&list), ["a\\046b.c\\001", "."]);
    }
}
