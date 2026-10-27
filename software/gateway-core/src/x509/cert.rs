//! Reading an X.509 certificate (DER): the fields the certificate manager
//! shows and checks. Nothing here verifies a signature.

use super::der::{self, oid, tag, tlv};
use super::time;
use core::fmt;

/// The parts of a certificate, borrowed from its DER bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Certificate<'a> {
    /// The whole certificate.
    pub der: &'a [u8],
    pub serial: &'a [u8],
    /// The issuer `Name`, header included (byte-compared with a subject to
    /// link a chain).
    pub issuer: &'a [u8],
    pub subject: &'a [u8],
    /// Validity, in Unix seconds.
    pub not_before: i64,
    pub not_after: i64,
    /// The whole `SubjectPublicKeyInfo` element.
    pub spki: &'a [u8],
    /// The key bits: an uncompressed SEC1 point for EC, a PKCS#1
    /// `RSAPublicKey` for RSA.
    pub public_key: &'a [u8],
    /// The content of the `Extensions` SEQUENCE, if there is one.
    extensions: Option<&'a [u8]>,
}

impl<'a> Certificate<'a> {
    /// Parses a certificate; `None` if it doesn't have the X.509 shape.
    /// Trailing bytes after the certificate are not allowed.
    pub fn parse(der: &'a [u8]) -> Option<Self> {
        let (t, certificate, rest) = tlv(der)?;
        if t != tag::SEQUENCE || !rest.is_empty() {
            return None;
        }
        let (t, tbs, _) = tlv(certificate)?;
        if t != tag::SEQUENCE {
            return None;
        }

        let mut rest = tbs;
        let (t, _, after) = tlv(rest)?;
        if t == tag::context(0) {
            rest = after;
        }
        let (t, serial, after) = tlv(rest)?;
        if t != tag::INTEGER {
            return None;
        }
        let (_signature, after) = der::element(after)?;
        let (issuer, after) = der::element(after)?;
        let (t, validity, after) = tlv(after)?;
        if t != tag::SEQUENCE {
            return None;
        }
        let (subject, after) = der::element(after)?;
        let (spki, after) = der::element(after)?;

        let (t1, from, more) = tlv(validity)?;
        let (t2, until, _) = tlv(more)?;
        let not_before = time::parse_asn1(t1, from)?;
        let not_after = time::parse_asn1(t2, until)?;

        let (t, spki_body, _) = tlv(spki)?;
        if t != tag::SEQUENCE {
            return None;
        }
        let (_algorithm, key) = der::element(spki_body)?;
        let (t, bits, _) = tlv(key)?;
        if t != tag::BIT_STRING || bits.len() < 2 || bits[0] != 0 {
            return None;
        }

        // Optional issuerUniqueID [1], subjectUniqueID [2], extensions [3].
        let mut extensions = None;
        let mut rest = after;
        while !rest.is_empty() {
            let (t, content, after) = tlv(rest)?;
            if t == tag::context(3) {
                let (t, list, _) = tlv(content)?;
                if t != tag::SEQUENCE {
                    return None;
                }
                extensions = Some(list);
            }
            rest = after;
        }

        Some(Self {
            der,
            serial,
            issuer,
            subject,
            not_before,
            not_after,
            spki,
            public_key: &bits[1..],
            extensions,
        })
    }

    /// The value (the content of the `extnValue` OCTET STRING) of the
    /// extension `id`.
    pub fn extension(&self, id: &[u8]) -> Option<&'a [u8]> {
        let mut rest = self.extensions?;
        while !rest.is_empty() {
            let (_, extension, after) = tlv(rest).filter(|(t, _, _)| *t == tag::SEQUENCE)?;
            let (t, ext_id, more) = tlv(extension)?;
            if t == tag::OID && ext_id == id {
                let (t, _, after_critical) = tlv(more)?;
                let value_part = if t == tag::BOOLEAN {
                    after_critical
                } else {
                    more
                };
                let (t, value, _) = tlv(value_part)?;
                return (t == tag::OCTET_STRING).then_some(value);
            }
            rest = after;
        }
        None
    }

    /// The DNS names in the subject alternative name extension.
    pub fn dns_names(&self) -> DnsNames<'a> {
        let names = self
            .extension(oid::SUBJECT_ALT_NAME)
            .and_then(|value| tlv(value))
            .filter(|(t, _, _)| *t == tag::SEQUENCE)
            .map_or(&[][..], |(_, list, _)| list);
        DnsNames { rest: names }
    }

    pub fn is_self_issued(&self) -> bool {
        self.issuer == self.subject
    }

    /// Is the subject a certificate authority (basic constraints `cA`)?
    pub fn is_ca(&self) -> bool {
        self.extension(oid::BASIC_CONSTRAINTS)
            .and_then(tlv)
            .and_then(|(_, body, _)| tlv(body))
            .is_some_and(|(t, value, _)| t == tag::BOOLEAN && value.first() == Some(&0xFF))
    }

    /// Is this a P-256 key (the only kind the device holds)?
    pub fn has_p256_key(&self) -> bool {
        let Some((_, body, _)) = tlv(self.spki) else {
            return false;
        };
        let Some((_, algorithm, _)) = tlv(body) else {
            return false;
        };
        let Some((t, id, params)) = tlv(algorithm) else {
            return false;
        };
        t == tag::OID
            && id == oid::EC_PUBLIC_KEY
            && tlv(params).is_some_and(|(t, curve, _)| t == tag::OID && curve == oid::PRIME256V1)
    }
}

/// Iterator over the `dNSName` entries of a `GeneralNames` SEQUENCE.
pub struct DnsNames<'a> {
    rest: &'a [u8],
}

impl<'a> Iterator for DnsNames<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        while !self.rest.is_empty() {
            let (t, value, after) = tlv(self.rest)?;
            self.rest = after;
            // dNSName is [2] IMPLICIT IA5String.
            if t == 0x82 {
                if let Ok(name) = core::str::from_utf8(value) {
                    return Some(name);
                }
            }
        }
        None
    }
}

/// The attributes of a `Name`, as `(short label, value)` in order, e.g.
/// `("CN", "gateway")`. Unknown attribute types are skipped.
pub fn name_attributes(name: &[u8]) -> NameAttributes<'_> {
    let body = tlv(name)
        .filter(|(t, _, _)| *t == tag::SEQUENCE)
        .map_or(&[][..], |(_, body, _)| body);
    NameAttributes {
        sets: body,
        current: &[],
    }
}

pub struct NameAttributes<'a> {
    sets: &'a [u8],
    current: &'a [u8],
}

impl<'a> Iterator for NameAttributes<'a> {
    type Item = (&'static str, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.current.is_empty() {
                let (t, set, after) = tlv(self.sets)?;
                self.sets = after;
                if t == tag::SET {
                    self.current = set;
                }
                continue;
            }
            let (_, attribute, after) = tlv(self.current)?;
            self.current = after;
            let Some((tag::OID, id, value)) = tlv(attribute) else {
                continue;
            };
            let Some((_, text, _)) = tlv(value) else {
                continue;
            };
            let label = match id {
                oid::COMMON_NAME => "CN",
                oid::ORGANIZATION => "O",
                oid::ORGANIZATIONAL_UNIT => "OU",
                oid::COUNTRY => "C",
                oid::LOCALITY => "L",
                oid::STATE => "ST",
                _ => continue,
            };
            if let Ok(text) = core::str::from_utf8(text) {
                return Some((label, text));
            }
        }
    }
}

/// The common name of a `Name`, if it has one.
pub fn common_name(name: &[u8]) -> Option<&str> {
    name_attributes(name).find_map(|(label, value)| (label == "CN").then_some(value))
}

/// Formats a `Name` as `CN=…, O=…`.
pub struct DisplayName<'a>(pub &'a [u8]);

impl fmt::Display for DisplayName<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, (label, value)) in name_attributes(self.0).enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{label}={value}")?;
        }
        Ok(())
    }
}

/// The certificates in a concatenation of DER certificates (the way the
/// gateway stores a chain), each as its own slice.
pub fn split_concatenated(mut der: &[u8]) -> impl Iterator<Item = &[u8]> {
    core::iter::from_fn(move || {
        let (element, rest) = der::element(der)?;
        der = rest;
        Some(element)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::x509::fixtures::*;
    use std::string::String;
    use std::vec::Vec;

    #[test]
    fn reads_an_openssl_leaf() {
        let der = unhex(LEAF);
        let cert = Certificate::parse(&der).unwrap();
        assert_eq!(cert.not_before, LEAF_NOT_BEFORE);
        assert_eq!(cert.not_after, LEAF_NOT_AFTER);
        assert_eq!(cert.public_key, unhex(LEAF_POINT).as_slice());
        assert!(cert.has_p256_key());
        assert!(!cert.is_self_issued());
        assert!(!cert.is_ca());
        let names: Vec<&str> = cert.dns_names().collect();
        assert_eq!(names, ["gw.example.com", "alt.example.com"]);
        assert_eq!(common_name(cert.subject), Some("gw.example.com"));
        assert_eq!(
            format!("{}", DisplayName(cert.issuer)),
            "CN=Home Test CA, O=Home"
        );
        assert_eq!(
            format!("{}", DisplayName(cert.subject)),
            "CN=gw.example.com, O=Home"
        );
    }

    #[test]
    fn reads_an_openssl_ca() {
        let der = unhex(CA);
        let cert = Certificate::parse(&der).unwrap();
        assert!(cert.is_self_issued());
        assert!(cert.is_ca());
        assert_eq!(cert.dns_names().count(), 0);
        let leaf_der = unhex(LEAF);
        let leaf = Certificate::parse(&leaf_der).unwrap();
        assert_eq!(leaf.issuer, cert.subject);
    }

    #[test]
    fn splits_a_concatenated_chain() {
        let mut chain = unhex(LEAF);
        chain.extend(unhex(CA));
        let parts: Vec<&[u8]> = split_concatenated(&chain).collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[1], unhex(CA).as_slice());
    }

    #[test]
    fn rejects_malformed_certificates() {
        let der = unhex(LEAF);
        assert!(Certificate::parse(&[]).is_none());
        for cut in [1, 10, 100, der.len() - 1] {
            assert!(Certificate::parse(&der[..cut]).is_none(), "cut {cut}");
        }
        let mut trailing = der.clone();
        trailing.push(0);
        assert!(Certificate::parse(&trailing).is_none());
        let _ = String::new();
    }
}
