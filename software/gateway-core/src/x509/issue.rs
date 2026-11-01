//! What the gateway writes itself, for its own P-256 key: the PKCS#10
//! certificate request (CSR) for the configured FQDN, the self-signed
//! certificate, and the SEC1 form of the private key that MbedTLS loads.
//!
//! Signing is left to the caller (`sign` gets the bytes to sign and returns
//! a DER `ECDSA-Sig-Value`), so no key or RNG ever passes through this crate.

use super::der::{oid, tag, Overflow, Writer};
use super::time;

/// A DER ECDSA signature (`SEQUENCE { r, s }`), at most 72 bytes for P-256.
pub type Signature = heapless::Vec<u8, 72>;

/// Longest common name X.509 allows (`ub-common-name`); a longer FQDN is
/// only put in the subject alternative name.
pub const COMMON_NAME_MAX: usize = 64;

/// Longest FQDN accepted (RFC 1035).
pub const FQDN_MAX: usize = 253;

/// Why something couldn't be written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueError {
    /// The output buffer is too small.
    Overflow,
    /// The signer failed.
    Signing,
    /// The FQDN is empty, too long or has characters a DNS name can't have.
    BadFqdn,
}

impl From<Overflow> for IssueError {
    fn from(_: Overflow) -> Self {
        IssueError::Overflow
    }
}

/// Is `fqdn` a host name a certificate can be issued for: dot-separated
/// labels of letters, digits and `-`, each 1–63 long, at most 253 in all?
pub fn valid_fqdn(fqdn: &str) -> bool {
    !fqdn.is_empty()
        && fqdn.len() <= FQDN_MAX
        && fqdn.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

fn spki(w: &mut Writer<'_>, point: &[u8; 65]) -> Result<(), Overflow> {
    w.nested(tag::SEQUENCE, |w| {
        w.nested(tag::SEQUENCE, |w| {
            w.oid(oid::EC_PUBLIC_KEY)?;
            w.oid(oid::PRIME256V1)
        })?;
        w.bit_string(point)
    })
}

/// The `SubjectPublicKeyInfo` (DER) of the key `point` (uncompressed
/// SEC1), as it is in the CSR and the certificates; its SHA-256 is the
/// key's `pin-sha256` (RFC 7469). Returns the length written to `out`.
pub fn public_key_info(out: &mut [u8], point: &[u8; 65]) -> Result<usize, IssueError> {
    let mut w = Writer::new(out);
    spki(&mut w, point)?;
    Ok(w.len())
}

/// `CN=<fqdn>`, or the empty name if the FQDN is too long for a CN.
fn name(w: &mut Writer<'_>, fqdn: &str) -> Result<(), Overflow> {
    w.nested(tag::SEQUENCE, |w| {
        if fqdn.len() > COMMON_NAME_MAX {
            return Ok(());
        }
        w.nested(tag::SET, |w| {
            w.nested(tag::SEQUENCE, |w| {
                w.oid(oid::COMMON_NAME)?;
                w.element(tag::UTF8_STRING, fqdn.as_bytes())
            })
        })
    })
}

/// One `Extension`.
fn extension(
    w: &mut Writer<'_>,
    id: &[u8],
    critical: bool,
    value: impl FnOnce(&mut Writer<'_>) -> Result<(), Overflow>,
) -> Result<(), Overflow> {
    w.nested(tag::SEQUENCE, |w| {
        w.oid(id)?;
        if critical {
            w.element(tag::BOOLEAN, &[0xFF])?;
        }
        w.nested(tag::OCTET_STRING, value)
    })
}

/// The subject alternative name `DNS:<fqdn>`. Critical when the subject is
/// empty (RFC 5280 4.2.1.6).
fn san(w: &mut Writer<'_>, fqdn: &str) -> Result<(), Overflow> {
    extension(
        w,
        oid::SUBJECT_ALT_NAME,
        fqdn.len() > COMMON_NAME_MAX,
        |w| w.nested(tag::SEQUENCE, |w| w.element(0x82, fqdn.as_bytes())),
    )
}

fn signature_algorithm(w: &mut Writer<'_>) -> Result<(), Overflow> {
    w.nested(tag::SEQUENCE, |w| w.oid(oid::ECDSA_WITH_SHA256))
}

/// Writes `SEQUENCE { <to be signed>, ecdsa-with-SHA256, BIT STRING sig }`,
/// the shape shared by certificates and CSRs.
fn signed(
    out: &mut [u8],
    to_be_signed: impl FnOnce(&mut Writer<'_>) -> Result<(), Overflow>,
    sign: impl FnOnce(&[u8]) -> Option<Signature>,
) -> Result<usize, IssueError> {
    // Write the to-be-signed part first, at the end of the buffer's
    // future content: build it once, sign, then wrap.
    let mut w = Writer::new(out);
    let mut failed = false;
    let mut sign = Some(sign);
    let result = w.nested(tag::SEQUENCE, |w| {
        let start = w.len();
        to_be_signed(w)?;
        let signature = (sign.take().unwrap())(&w.as_bytes()[start..]);
        let Some(signature) = signature else {
            failed = true;
            return Ok(());
        };
        signature_algorithm(w)?;
        w.bit_string(&signature)
    });
    if failed {
        return Err(IssueError::Signing);
    }
    result?;
    Ok(w.len())
}

/// A PKCS#10 request for `fqdn` and the key `point` (uncompressed SEC1),
/// asking for the subject alternative name `DNS:<fqdn>`. Returns the DER
/// length written to `out`.
pub fn csr(
    out: &mut [u8],
    fqdn: &str,
    point: &[u8; 65],
    sign: impl FnOnce(&[u8]) -> Option<Signature>,
) -> Result<usize, IssueError> {
    if !valid_fqdn(fqdn) {
        return Err(IssueError::BadFqdn);
    }
    signed(
        out,
        |w| {
            w.nested(tag::SEQUENCE, |w| {
                w.unsigned(&[0])?;
                name(w, fqdn)?;
                spki(w, point)?;
                // attributes [0] IMPLICIT SET OF Attribute
                w.nested(tag::context(0), |w| {
                    w.nested(tag::SEQUENCE, |w| {
                        w.oid(oid::EXTENSION_REQUEST)?;
                        w.nested(tag::SET, |w| w.nested(tag::SEQUENCE, |w| san(w, fqdn)))
                    })
                })
            })
        },
        sign,
    )
}

/// What goes into a self-signed certificate besides the key.
#[derive(Debug, Clone, Copy)]
pub struct SelfSigned<'a> {
    pub fqdn: &'a str,
    /// Positive serial number (the top bit is cleared).
    pub serial: [u8; 16],
    pub not_before: i64,
    pub not_after: i64,
}

/// A self-signed server certificate for `fqdn`: X.509 v3, not a CA, key
/// usage `digitalSignature`, extended key usage `serverAuth`, and the
/// subject alternative name. Returns the DER length written to `out`.
pub fn self_signed(
    out: &mut [u8],
    params: &SelfSigned<'_>,
    point: &[u8; 65],
    sign: impl FnOnce(&[u8]) -> Option<Signature>,
) -> Result<usize, IssueError> {
    if !valid_fqdn(params.fqdn) {
        return Err(IssueError::BadFqdn);
    }
    let mut serial = params.serial;
    serial[0] &= 0x7F;
    if serial.iter().all(|b| *b == 0) {
        serial[15] = 1;
    }
    signed(
        out,
        |w| {
            w.nested(tag::SEQUENCE, |w| {
                w.nested(tag::context(0), |w| w.unsigned(&[2]))?;
                w.unsigned(&serial)?;
                signature_algorithm(w)?;
                name(w, params.fqdn)?;
                w.nested(tag::SEQUENCE, |w| {
                    time::write_asn1(w, params.not_before)?;
                    time::write_asn1(w, params.not_after)
                })?;
                name(w, params.fqdn)?;
                spki(w, point)?;
                w.nested(tag::context(3), |w| {
                    w.nested(tag::SEQUENCE, |w| {
                        // basicConstraints: cA FALSE (the default, so empty).
                        extension(w, oid::BASIC_CONSTRAINTS, true, |w| {
                            w.nested(tag::SEQUENCE, |_| Ok(()))
                        })?;
                        // keyUsage: digitalSignature (bit 0).
                        extension(w, oid::KEY_USAGE, true, |w| {
                            w.element(tag::BIT_STRING, &[0x07, 0x80])
                        })?;
                        extension(w, oid::EXT_KEY_USAGE, false, |w| {
                            w.nested(tag::SEQUENCE, |w| w.oid(oid::SERVER_AUTH))
                        })?;
                        san(w, params.fqdn)
                    })
                })
            })
        },
        sign,
    )
}

/// The private key as a SEC1 `ECPrivateKey` (RFC 5915), the form MbedTLS'
/// `mbedtls_pk_parse_key` takes. `out` holds key material afterwards: the
/// caller must wipe it.
pub fn sec1_private_key(
    out: &mut [u8],
    scalar: &[u8; 32],
    point: &[u8; 65],
) -> Result<usize, IssueError> {
    let mut w = Writer::new(out);
    w.nested(tag::SEQUENCE, |w| {
        w.unsigned(&[1])?;
        w.element(tag::OCTET_STRING, scalar)?;
        w.nested(tag::context(0), |w| w.oid(oid::PRIME256V1))?;
        w.nested(tag::context(1), |w| w.bit_string(point))
    })?;
    Ok(w.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::x509::cert::Certificate;
    use crate::x509::der::tlv;
    use crate::x509::fixtures::*;
    use p256::ecdsa::signature::{Signer, Verifier};
    use p256::ecdsa::{Signature as EcSignature, SigningKey, VerifyingKey};
    use std::vec::Vec;

    fn key() -> (SigningKey, [u8; 65]) {
        let scalar = unhex(LEAF_KEY);
        let key = SigningKey::from_slice(&scalar).unwrap();
        let point: [u8; 65] = unhex(LEAF_POINT).try_into().unwrap();
        (key, point)
    }

    fn signer(key: &SigningKey) -> impl FnOnce(&[u8]) -> Option<Signature> + '_ {
        move |data| {
            let signature: EcSignature = key.sign(data);
            Signature::from_slice(signature.to_der().as_bytes()).ok()
        }
    }

    /// Checks the outer signature of a certificate or CSR against `point`.
    fn verify_outer(der: &[u8], point: &[u8; 65]) {
        let (_, body, _) = tlv(der).unwrap();
        let (_, _, after_tbs) = tlv(body).unwrap();
        let tbs = &body[..body.len() - after_tbs.len()];
        let (_, _, after_alg) = tlv(after_tbs).unwrap();
        let (_, bits, _) = tlv(after_alg).unwrap();
        let signature = EcSignature::from_der(&bits[1..]).unwrap();
        let key = VerifyingKey::from_sec1_bytes(point).unwrap();
        key.verify(tbs, &signature).unwrap();
    }

    #[test]
    fn self_signed_certificate_reads_back_and_verifies() {
        let (key, point) = key();
        let mut out = [0u8; 1024];
        let params = SelfSigned {
            fqdn: "heizung-keller.devices.example.com",
            serial: [0xFF; 16],
            not_before: 1_793_000_000,
            not_after: 1_793_000_000 + 10 * 365 * 86_400,
        };
        let len = self_signed(&mut out, &params, &point, signer(&key)).unwrap();
        let der = &out[..len];
        verify_outer(der, &point);

        let cert = Certificate::parse(der).unwrap();
        assert!(cert.is_self_issued());
        assert!(!cert.is_ca());
        assert!(cert.has_p256_key());
        assert_eq!(cert.public_key, &point);
        assert_eq!(cert.not_before, params.not_before);
        assert_eq!(cert.not_after, params.not_after);
        assert_eq!(
            cert.dns_names().collect::<Vec<_>>(),
            ["heizung-keller.devices.example.com"]
        );
        assert_eq!(
            crate::x509::cert::common_name(cert.subject),
            Some("heizung-keller.devices.example.com")
        );
        // The top bit was cleared, so the serial is 16 bytes, positive.
        assert_eq!(cert.serial.len(), 16);
        assert_eq!(cert.serial[0], 0x7F);
    }

    #[test]
    fn a_long_fqdn_goes_only_into_the_san() {
        let (key, point) = key();
        let fqdn = "a-rather-long-host-name-for-a-heating-controller.in-the-basement.example.com";
        assert!(fqdn.len() > COMMON_NAME_MAX);
        let mut out = [0u8; 1024];
        let params = SelfSigned {
            fqdn,
            serial: [1; 16],
            not_before: 0,
            not_after: 2_600_000_000,
        };
        let len = self_signed(&mut out, &params, &point, signer(&key)).unwrap();
        let cert = Certificate::parse(&out[..len]).unwrap();
        assert_eq!(crate::x509::cert::common_name(cert.subject), None);
        assert_eq!(cert.dns_names().next(), Some(fqdn));
        verify_outer(&out[..len], &point);
    }

    #[test]
    fn csr_has_the_key_name_and_san_request() {
        let (key, point) = key();
        let mut out = [0u8; 512];
        let len = csr(&mut out, "gw.example.com", &point, signer(&key)).unwrap();
        let der = &out[..len];
        verify_outer(der, &point);

        // CertificationRequestInfo: version 0, subject, SPKI, attributes.
        let (_, body, _) = tlv(der).unwrap();
        let (_, info, _) = tlv(body).unwrap();
        let (_, version, rest) = tlv(info).unwrap();
        assert_eq!(version, &[0]);
        let (subject, rest) = crate::x509::der::element(rest).unwrap();
        assert_eq!(
            crate::x509::cert::common_name(subject),
            Some("gw.example.com")
        );
        let (spki, rest) = crate::x509::der::element(rest).unwrap();
        assert!(spki.ends_with(&point));
        let mut own = [0u8; 128];
        let n = public_key_info(&mut own, &point).unwrap();
        assert_eq!(&own[..n], spki);
        let (t, attributes, _) = tlv(rest).unwrap();
        assert_eq!(t, 0xA0);
        let needle = b"gw.example.com";
        assert!(attributes.windows(needle.len()).any(|w| w == needle));
        assert!(attributes
            .windows(oid::EXTENSION_REQUEST.len())
            .any(|w| w == oid::EXTENSION_REQUEST));
    }

    #[test]
    fn bad_fqdns_are_refused() {
        let (key, point) = key();
        let mut out = [0u8; 512];
        for fqdn in ["", "a..b", "-a.b", "a b.c", "x.", "ümlaut.de"] {
            assert_eq!(
                csr(&mut out, fqdn, &point, signer(&key)),
                Err(IssueError::BadFqdn),
                "{fqdn}"
            );
        }
        assert!(valid_fqdn("gateway"));
        assert!(valid_fqdn("heizung-keller.devices.example.com"));
    }

    #[test]
    fn a_failing_signer_and_a_small_buffer_are_errors() {
        let (_, point) = key();
        let mut out = [0u8; 512];
        assert_eq!(
            csr(&mut out, "gw.example.com", &point, |_| None),
            Err(IssueError::Signing)
        );
        let (key, _) = key();
        let mut small = [0u8; 100];
        assert_eq!(
            csr(&mut small, "gw.example.com", &point, signer(&key)),
            Err(IssueError::Overflow)
        );
    }

    #[test]
    fn sec1_key_has_the_rfc5915_shape() {
        let (_, point) = key();
        let scalar: [u8; 32] = unhex(LEAF_KEY).try_into().unwrap();
        let mut out = [0u8; 128];
        let len = sec1_private_key(&mut out, &scalar, &point).unwrap();
        // 30 77 02 01 01 04 20 <32> a0 0a 06 08 <prime256v1> a1 44 03 42 00 <65>
        assert_eq!(len, 121);
        assert_eq!(&out[..7], &[0x30, 0x77, 0x02, 0x01, 0x01, 0x04, 0x20]);
        assert_eq!(&out[7..39], &scalar);
        assert_eq!(&out[39..41], &[0xA0, 0x0A]);
        assert_eq!(&out[41..43], &[0x06, 0x08]);
        assert_eq!(&out[51..54], &[0xA1, 0x44, 0x03]);
        assert_eq!(&out[54..56], &[0x42, 0x00]);
        assert_eq!(&out[56..121], &point);
    }
}
