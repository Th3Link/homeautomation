//! The certificates in a "certs-only" PKCS#7 `SignedData` (RFC 5652), the
//! form an EST server returns an issued certificate in (RFC 7030 4.2.3).

use super::der::{self, oid, tag, tlv};

/// The certificates in a DER `ContentInfo` holding `SignedData`, in the
/// order they appear (which says nothing about the chain order). `None` if
/// it is not that shape.
pub fn certificates(der: &[u8]) -> Option<impl Iterator<Item = &[u8]>> {
    let (t, content_info, _) = tlv(der)?;
    if t != tag::SEQUENCE {
        return None;
    }
    let (t, content_type, rest) = tlv(content_info)?;
    if t != tag::OID || content_type != oid::SIGNED_DATA {
        return None;
    }
    let (t, explicit, _) = tlv(rest)?;
    if t != tag::context(0) {
        return None;
    }
    let (t, signed_data, _) = tlv(explicit)?;
    if t != tag::SEQUENCE {
        return None;
    }
    // version, digestAlgorithms, encapContentInfo, [0] certificates.
    let (_, rest) = der::element(signed_data)?;
    let (_, rest) = der::element(rest)?;
    let (_, rest) = der::element(rest)?;
    let (t, mut list, _) = tlv(rest)?;
    if t != tag::context(0) {
        return None;
    }
    Some(core::iter::from_fn(move || loop {
        let (t, _, _) = tlv(list)?;
        let (element, after) = der::element(list)?;
        list = after;
        // Skip the other CertificateChoices (attribute certificates…).
        if t == tag::SEQUENCE {
            return Some(element);
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::x509::fixtures::*;
    use std::vec::Vec;

    #[test]
    fn finds_both_certificates_of_an_openssl_bundle() {
        let der = unhex(P7_CERTS_ONLY);
        let certs: Vec<&[u8]> = certificates(&der).unwrap().collect();
        assert_eq!(certs.len(), 2);
        assert_eq!(certs[0], unhex(CA).as_slice());
        assert_eq!(certs[1], unhex(LEAF).as_slice());
    }

    #[test]
    fn rejects_other_things() {
        assert!(certificates(&unhex(LEAF)).is_none());
        assert!(certificates(&[]).is_none());
        let der = unhex(P7_CERTS_ONLY);
        assert!(certificates(&der[..40]).is_none());
    }
}
