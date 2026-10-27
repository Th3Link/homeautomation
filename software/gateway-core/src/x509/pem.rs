//! Base64 and PEM (`-----BEGIN …-----` armour around base64 DER).

use core::fmt;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// The base64 text is malformed, or the output buffer is too small.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Invalid;

fn sextet(c: u8) -> Option<u32> {
    Some(match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => return None,
    } as u32)
}

/// Decodes base64 into `out`, skipping whitespace; returns the length.
pub fn decode(text: &[u8], out: &mut [u8]) -> Result<usize, Invalid> {
    let mut len = 0;
    let mut acc = 0u32;
    let mut bits = 0;
    let mut padding = 0;
    for &c in text {
        if c.is_ascii_whitespace() {
            continue;
        }
        if c == b'=' {
            padding += 1;
            continue;
        }
        // Nothing may follow the padding.
        if padding > 0 {
            return Err(Invalid);
        }
        acc = (acc << 6) | sextet(c).ok_or(Invalid)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            *out.get_mut(len).ok_or(Invalid)? = (acc >> bits) as u8;
            len += 1;
        }
    }
    if padding > 2 || bits >= 6 {
        return Err(Invalid);
    }
    Ok(len)
}

/// Base64 of some bytes, for formatting; `line` breaks the text every that
/// many characters (0: one line).
pub struct Base64<'a> {
    pub bytes: &'a [u8],
    pub line: usize,
}

impl fmt::Display for Base64<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut column = 0;
        for chunk in self.bytes.chunks(3) {
            let n = chunk
                .iter()
                .enumerate()
                .fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
            for i in 0..4 {
                if self.line > 0 && column == self.line {
                    f.write_str("\n")?;
                    column = 0;
                }
                let c = if i <= chunk.len() {
                    ALPHABET[(n >> (18 - 6 * i) & 0x3F) as usize]
                } else {
                    b'='
                };
                fmt::Write::write_char(f, char::from(c))?;
                column += 1;
            }
        }
        Ok(())
    }
}

/// A PEM block: `label` (e.g. `CERTIFICATE`) around base64 `der`.
pub struct Pem<'a> {
    pub label: &'a str,
    pub der: &'a [u8],
}

impl fmt::Display for Pem<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "-----BEGIN {}-----", self.label)?;
        writeln!(
            f,
            "{}",
            Base64 {
                bytes: self.der,
                line: 64
            }
        )?;
        writeln!(f, "-----END {}-----", self.label)
    }
}

/// The PEM blocks in a text, as `(label, base64 body)`. Text outside the
/// blocks (comments, `Bag Attributes` from openssl) is skipped.
pub fn blocks(text: &str) -> Blocks<'_> {
    Blocks { rest: text }
}

pub struct Blocks<'a> {
    rest: &'a str,
}

impl<'a> Iterator for Blocks<'a> {
    type Item = (&'a str, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        let start = self.rest.find("-----BEGIN ")?;
        let after = &self.rest[start + 11..];
        let label_end = after.find("-----")?;
        let label = &after[..label_end];
        let body = &after[label_end + 5..];
        let end_marker = body.find("-----END ")?;
        let content = &body[..end_marker];
        let tail = &body[end_marker + 9..];
        let tail_end = tail.find("-----").map_or(tail.len(), |i| i + 5);
        self.rest = &tail[tail_end..];
        Some((label, content))
    }
}

/// Is this text PEM rather than raw DER?
pub fn is_pem(data: &[u8]) -> bool {
    data.windows(11).any(|w| w == b"-----BEGIN ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::fmt::Write;

    fn encode(bytes: &[u8]) -> heapless::String<128> {
        let mut s = heapless::String::new();
        write!(s, "{}", Base64 { bytes, line: 0 }).unwrap();
        s
    }

    #[test]
    fn rfc4648_vectors() {
        for (plain, coded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(encode(plain.as_bytes()), coded);
            let mut out = [0u8; 8];
            let n = decode(coded.as_bytes(), &mut out).unwrap();
            assert_eq!(&out[..n], plain.as_bytes());
        }
    }

    #[test]
    fn decoding_skips_whitespace_and_rejects_junk() {
        let mut out = [0u8; 8];
        assert_eq!(decode(b"Zm9v\r\nYmFy\n", &mut out), Ok(6));
        assert_eq!(decode(b"Zm9v*mFy", &mut out), Err(Invalid));
        assert_eq!(decode(b"Zg==Zg==", &mut out), Err(Invalid));
        assert_eq!(decode(b"Z", &mut out), Err(Invalid));
        assert_eq!(decode(b"Zm9vYmFy", &mut out[..5]), Err(Invalid));
    }

    #[test]
    fn line_breaks() {
        let mut s = heapless::String::<256>::new();
        write!(
            s,
            "{}",
            Base64 {
                bytes: &[0xFF; 60],
                line: 64
            }
        )
        .unwrap();
        let lines: heapless::Vec<&str, 4> = s.split('\n').collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].len(), 64);
        assert_eq!(lines[1].len(), 16);
    }

    #[test]
    fn pem_round_trip_with_several_blocks() {
        let mut text = heapless::String::<512>::new();
        writeln!(text, "subject=CN = leaf").unwrap();
        write!(
            text,
            "{}",
            Pem {
                label: "CERTIFICATE",
                der: &[1, 2, 3, 4, 5]
            }
        )
        .unwrap();
        write!(
            text,
            "{}",
            Pem {
                label: "CERTIFICATE",
                der: &[9; 70]
            }
        )
        .unwrap();
        write!(
            text,
            "{}",
            Pem {
                label: "EC PRIVATE KEY",
                der: &[7]
            }
        )
        .unwrap();

        let found: heapless::Vec<(&str, &str), 4> = blocks(&text).collect();
        assert_eq!(found.len(), 3);
        assert_eq!(found[0].0, "CERTIFICATE");
        assert_eq!(found[2].0, "EC PRIVATE KEY");
        let mut out = [0u8; 80];
        assert_eq!(decode(found[1].1.as_bytes(), &mut out), Ok(70));
        assert!(is_pem(text.as_bytes()));
        assert!(!is_pem(&[0x30, 0x82, 1, 0]));
    }
}
