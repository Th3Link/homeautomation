//! The bits of DER (the binary encoding of X.509) the gateway needs: a
//! reader that splits off one element at a time, and a writer for the
//! certificates and requests it builds itself.

/// Splits the DER element at the start of `input` into
/// `(tag, content, rest)`, or `None` if it is cut off or its length is
/// longer than 3 bytes (16 MiB, far more than anything here).
pub fn tlv(input: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    let (&tag, rest) = input.split_first()?;
    let (&first, rest) = rest.split_first()?;
    let (len, rest) = if first < 0x80 {
        (usize::from(first), rest)
    } else {
        let count = usize::from(first & 0x7F);
        if count == 0 || count > 3 || rest.len() < count {
            return None;
        }
        let len = rest[..count]
            .iter()
            .fold(0usize, |len, byte| (len << 8) | usize::from(*byte));
        (len, &rest[count..])
    };
    if rest.len() < len {
        return None;
    }
    Some((tag, &rest[..len], &rest[len..]))
}

/// The DER element at the start of `input`, header included, and the rest.
pub fn element(input: &[u8]) -> Option<(&[u8], &[u8])> {
    let (_, _, rest) = tlv(input)?;
    Some((&input[..input.len() - rest.len()], rest))
}

/// Tags used here.
pub mod tag {
    pub const BOOLEAN: u8 = 0x01;
    pub const INTEGER: u8 = 0x02;
    pub const BIT_STRING: u8 = 0x03;
    pub const OCTET_STRING: u8 = 0x04;
    pub const OID: u8 = 0x06;
    pub const UTF8_STRING: u8 = 0x0C;
    pub const PRINTABLE_STRING: u8 = 0x13;
    pub const IA5_STRING: u8 = 0x16;
    pub const UTC_TIME: u8 = 0x17;
    pub const GENERALIZED_TIME: u8 = 0x18;
    pub const SEQUENCE: u8 = 0x30;
    pub const SET: u8 = 0x31;
    /// `[n]`, constructed (explicit tagging, or an implicitly tagged
    /// SEQUENCE/SET).
    pub const fn context(n: u8) -> u8 {
        0xA0 | n
    }
}

/// The buffer given to a [`Writer`] is full.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Overflow;

/// Writes DER into a fixed buffer. Constructed elements are written with
/// [`Writer::nested`], which fills in the length once the content is known.
pub struct Writer<'a> {
    buf: &'a mut [u8],
    len: usize,
}

/// Room reserved for the length of a constructed element before its content
/// is known: the `0x83` form, up to 16 MiB.
const LENGTH_RESERVE: usize = 4;

impl<'a> Writer<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, len: 0 }
    }

    /// What has been written.
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn raw(&mut self, bytes: &[u8]) -> Result<(), Overflow> {
        let end = self.len + bytes.len();
        self.buf
            .get_mut(self.len..end)
            .ok_or(Overflow)?
            .copy_from_slice(bytes);
        self.len = end;
        Ok(())
    }

    fn length(&mut self, len: usize) -> Result<(), Overflow> {
        match len {
            0..=0x7F => self.raw(&[len as u8]),
            0x80..=0xFF => self.raw(&[0x81, len as u8]),
            0x100..=0xFFFF => self.raw(&[0x82, (len >> 8) as u8, len as u8]),
            _ => self.raw(&[0x83, (len >> 16) as u8, (len >> 8) as u8, len as u8]),
        }
    }

    /// A primitive element.
    pub fn element(&mut self, tag: u8, content: &[u8]) -> Result<(), Overflow> {
        self.raw(&[tag])?;
        self.length(content.len())?;
        self.raw(content)
    }

    /// A constructed element whose content `content` writes.
    pub fn nested(
        &mut self,
        tag: u8,
        content: impl FnOnce(&mut Self) -> Result<(), Overflow>,
    ) -> Result<(), Overflow> {
        self.raw(&[tag])?;
        let header = self.len;
        self.raw(&[0; LENGTH_RESERVE])?;
        let start = self.len;
        content(self)?;
        let content_len = self.len - start;

        // Write the real length where the reserve is, then close the gap.
        self.len = header;
        self.length(content_len)?;
        let used = self.len - header;
        self.buf
            .copy_within(start..start + content_len, header + used);
        self.len = header + used + content_len;
        Ok(())
    }

    /// An INTEGER from a big-endian unsigned number.
    pub fn unsigned(&mut self, bytes: &[u8]) -> Result<(), Overflow> {
        let first = bytes.iter().position(|b| *b != 0).unwrap_or(bytes.len());
        let bytes = &bytes[first..];
        self.raw(&[tag::INTEGER])?;
        match bytes.first() {
            None => self.raw(&[1, 0]),
            Some(top) if *top >= 0x80 => {
                self.length(bytes.len() + 1)?;
                self.raw(&[0])?;
                self.raw(bytes)
            }
            Some(_) => {
                self.length(bytes.len())?;
                self.raw(bytes)
            }
        }
    }

    pub fn oid(&mut self, encoded: &[u8]) -> Result<(), Overflow> {
        self.element(tag::OID, encoded)
    }

    /// A BIT STRING of whole bytes.
    pub fn bit_string(&mut self, bytes: &[u8]) -> Result<(), Overflow> {
        self.raw(&[tag::BIT_STRING])?;
        self.length(bytes.len() + 1)?;
        self.raw(&[0])?;
        self.raw(bytes)
    }
}

/// Object identifiers (their DER content bytes).
pub mod oid {
    pub const EC_PUBLIC_KEY: &[u8] = &[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x02, 0x01];
    pub const PRIME256V1: &[u8] = &[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x03, 0x01, 0x07];
    pub const ECDSA_WITH_SHA256: &[u8] = &[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x04, 0x03, 0x02];
    pub const COMMON_NAME: &[u8] = &[0x55, 0x04, 0x03];
    pub const COUNTRY: &[u8] = &[0x55, 0x04, 0x06];
    pub const LOCALITY: &[u8] = &[0x55, 0x04, 0x07];
    pub const STATE: &[u8] = &[0x55, 0x04, 0x08];
    pub const ORGANIZATION: &[u8] = &[0x55, 0x04, 0x0A];
    pub const ORGANIZATIONAL_UNIT: &[u8] = &[0x55, 0x04, 0x0B];
    pub const KEY_USAGE: &[u8] = &[0x55, 0x1D, 0x0F];
    pub const SUBJECT_ALT_NAME: &[u8] = &[0x55, 0x1D, 0x11];
    pub const BASIC_CONSTRAINTS: &[u8] = &[0x55, 0x1D, 0x13];
    pub const EXT_KEY_USAGE: &[u8] = &[0x55, 0x1D, 0x25];
    pub const SERVER_AUTH: &[u8] = &[0x2B, 0x06, 0x01, 0x05, 0x05, 0x07, 0x03, 0x01];
    pub const EXTENSION_REQUEST: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x0E];
    pub const SIGNED_DATA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x02];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_and_long_lengths_round_trip() {
        for len in [0usize, 1, 127, 128, 255, 256, 1000, 70_000] {
            let mut buf = [0u8; 70_100];
            let mut w = Writer::new(&mut buf);
            let content = [0x5A; 70_000];
            w.nested(tag::SEQUENCE, |w| w.raw(&content[..len])).unwrap();
            let (t, c, rest) = tlv(w.as_bytes()).unwrap();
            assert_eq!(
                (t, c.len(), rest.len()),
                (tag::SEQUENCE, len, 0),
                "len {len}"
            );
            assert!(c.iter().all(|b| *b == 0x5A));
        }
    }

    #[test]
    fn length_uses_the_shortest_form() {
        let mut buf = [0u8; 300];
        let mut w = Writer::new(&mut buf);
        w.nested(tag::SEQUENCE, |w| w.raw(&[1, 2, 3])).unwrap();
        assert_eq!(w.as_bytes(), &[0x30, 3, 1, 2, 3]);

        let mut w = Writer::new(&mut buf);
        w.nested(tag::SEQUENCE, |w| w.raw(&[7; 200])).unwrap();
        assert_eq!(&w.as_bytes()[..3], &[0x30, 0x81, 200]);
        assert_eq!(w.len(), 203);
    }

    #[test]
    fn nested_elements_close_their_gaps() {
        let mut buf = [0u8; 64];
        let mut w = Writer::new(&mut buf);
        w.nested(tag::SEQUENCE, |w| {
            w.nested(tag::SET, |w| w.unsigned(&[5]))?;
            w.oid(oid::COMMON_NAME)
        })
        .unwrap();
        assert_eq!(
            w.as_bytes(),
            &[0x30, 10, 0x31, 3, 0x02, 1, 5, 0x06, 3, 0x55, 0x04, 0x03]
        );
    }

    #[test]
    fn unsigned_integers_are_minimal_and_positive() {
        let mut buf = [0u8; 16];
        let mut w = Writer::new(&mut buf);
        w.unsigned(&[0, 0, 0x7F]).unwrap();
        assert_eq!(w.as_bytes(), &[2, 1, 0x7F]);
        let mut w = Writer::new(&mut buf);
        w.unsigned(&[0x80, 1]).unwrap();
        assert_eq!(w.as_bytes(), &[2, 3, 0, 0x80, 1]);
        let mut w = Writer::new(&mut buf);
        w.unsigned(&[0, 0]).unwrap();
        assert_eq!(w.as_bytes(), &[2, 1, 0]);
    }

    #[test]
    fn a_full_buffer_is_reported() {
        let mut buf = [0u8; 4];
        let mut w = Writer::new(&mut buf);
        assert_eq!(w.raw(&[1, 2, 3, 4, 5]), Err(Overflow));
        let mut w = Writer::new(&mut buf);
        assert_eq!(w.nested(tag::SEQUENCE, |w| w.raw(&[1, 2])), Err(Overflow));
    }

    #[test]
    fn cut_off_elements_are_not_read() {
        assert_eq!(tlv(&[0x30]), None);
        assert_eq!(tlv(&[0x30, 5, 1]), None);
        assert_eq!(tlv(&[0x30, 0x84, 0, 0, 0, 1, 0]), None);
        assert_eq!(
            element(&[0x02, 1, 9, 0xFF]),
            Some((&[0x02, 1, 9][..], &[0xFF][..]))
        );
    }
}
