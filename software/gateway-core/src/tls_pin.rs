//! The pure parts of pinning a broker's TLS identity (see
//! `gateway-hardware::tls`): finding the public key in a DER certificate,
//! and building the message a TLS 1.3 server signs to prove it owns that
//! key.
//!
//! The gateway pins the *public key* of the broker's certificate rather
//! than the certificate: a server is accepted if the key it presents is the
//! pinned one and it proves, with the handshake signature, that it holds
//! the private key. That also survives the certificate being renewed under
//! the same key, and needs no certificate authority, no host name and no
//! clock.

use crate::x509::der::tlv;
use heapless::Vec;

/// The public key of an X.509 certificate (DER).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublicKey<'a> {
    /// The whole `SubjectPublicKeyInfo` element, header included: what
    /// identifies the key (algorithm and curve/parameters included).
    pub info: &'a [u8],
    /// The key itself, as the `BIT STRING` holds it: an uncompressed SEC1
    /// point for ECDSA, a PKCS#1 `RSAPublicKey` for RSA.
    pub key: &'a [u8],
}

/// Finds the `SubjectPublicKeyInfo` in an X.509 certificate (DER), or
/// `None` if the certificate doesn't have the expected shape.
pub fn public_key(certificate: &[u8]) -> Option<PublicKey<'_>> {
    let (_, certificate, _) = tlv(certificate)?;
    let (_, tbs, _) = tlv(certificate)?;

    // tbsCertificate: [0] version (optional), serialNumber, signature,
    // issuer, validity, subject, subjectPublicKeyInfo, ...
    let mut rest = tbs;
    let (tag, _, after) = tlv(rest)?;
    if tag == 0xA0 {
        rest = after;
    }
    for _ in 0..5 {
        let (_, _, after) = tlv(rest)?;
        rest = after;
    }

    let (tag, info_body, after) = tlv(rest)?;
    if tag != 0x30 {
        return None;
    }
    let info = &rest[..rest.len() - after.len()];

    let (_, _algorithm, after_algorithm) = tlv(info_body)?;
    let (tag, bits, _) = tlv(after_algorithm)?;
    // The first content byte of a BIT STRING counts its unused bits.
    if tag != 0x03 || bits.len() < 2 || bits[0] != 0 {
        return None;
    }
    Some(PublicKey {
        info,
        key: &bits[1..],
    })
}

/// Do two certificates (DER) carry the same public key?
pub fn same_public_key(a: &[u8], b: &[u8]) -> bool {
    match (public_key(a), public_key(b)) {
        (Some(a), Some(b)) => a.info == b.info,
        _ => false,
    }
}

/// The octets a TLS 1.3 server signs in its `CertificateVerify` message
/// (RFC 8446 section 4.4.3): 64 spaces, a context string, a zero byte and
/// the handshake transcript hash up to the `Certificate` message. `hash` is
/// 32 bytes for SHA-256 and 48 for SHA-384; longer ones are cut off.
pub fn server_verify_message(hash: &[u8]) -> Vec<u8, 146> {
    const CONTEXT: &[u8] = b"TLS 1.3, server CertificateVerify\x00";
    let mut message = Vec::new();
    let _ = message.resize(64, 0x20);
    let _ = message.extend_from_slice(CONTEXT);
    let _ = message.extend_from_slice(&hash[..hash.len().min(48)]);
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    // Self-signed certificates made with `openssl req -x509` (an ECDSA P-256
    // one and an RSA-2048 one), and their SubjectPublicKeyInfo as
    // `openssl pkey -pubin -outform DER` prints it.
    const EC_CERT: &str = "3082017c30820123a00302010202143b2550101379eb080c3048d62f83c682971efdd9300a06082a8648ce3d04030230143112301006035504030c096777746573742d6563301e170d3236313032343134343735385a170d3237313032343134343735385a30143112301006035504030c096777746573742d65633059301306072a8648ce3d020106082a8648ce3d03010703420004468e3209ef084f4bfa51f8dadf28153c008f833bfc9ffb218853d1f3c45bb77988764dbc0df34e16ee35cae82048f7658c97bfcb950c42455ccf8190429cc97ba3533051301d0603551d0e041604145652e91366492930f8219f862d7bc3b79c08a9a0301f0603551d230418301680145652e91366492930f8219f862d7bc3b79c08a9a0300f0603551d130101ff040530030101ff300a06082a8648ce3d04030203470030440220654644e5746fa55af0e7208ef54e9f37dfec5b8b222e2fb92954c7a45e9db21e022049b269cac0d596c8a49837ae67b5358d2a5d87a3f10dff37031e4780b9e0ee3d";
    const EC_SPKI: &str = "3059301306072a8648ce3d020106082a8648ce3d03010703420004468e3209ef084f4bfa51f8dadf28153c008f833bfc9ffb218853d1f3c45bb77988764dbc0df34e16ee35cae82048f7658c97bfcb950c42455ccf8190429cc97b";
    const RSA_CERT: &str = "3082030b308201f3a003020102021454da12d2cee42c8bb5997ea6c4c18397bdfa3dab300d06092a864886f70d01010b050030153113301106035504030c0a6777746573742d727361301e170d3236313032343134343735385a170d3237313032343134343735385a30153113301106035504030c0a6777746573742d72736130820122300d06092a864886f70d01010105000382010f003082010a0282010100d583fa9ba6c986b5619f70c66377fc7e31ac08553da63c8f04b8dd7a8a4de656b047f2064fa28706a42139f21b33a53b24f5bd27c8dea735b59313b4982b998deeafc9a469115c87c1a1a777f312fbda847352288c21e518ba2d47359dd3ba58f9ccf042dbe1ae6091415a9a4267dd8183bc8bee374b4fb275b8986ed29078d22e4e413179a3d34a55199d4f94bc4d928558bad35f5a25905e8550db3fd53aafc50792fe44b6d5e67d3fc5ec6e989646c2858877bca8aa2ad254a9b55ab7ec7f1aa8fb5d53387452e0ae80a1003f65207f62c53dae67a154fda58b50be9957d5eb24dc86850ed5c9057e620aae9fd8078301048c3a659bcfa180a7c9049a31110203010001a3533051301d0603551d0e041604145b1961d45b2c6f90b7f079f99aa0c5fc709a19e3301f0603551d230418301680145b1961d45b2c6f90b7f079f99aa0c5fc709a19e3300f0603551d130101ff040530030101ff300d06092a864886f70d01010b0500038201010044817b642c7c7ebfa3167d6b66f197003f4179cc3cf2712b9f6a4c4ebf918059b9fe27622bdaf45ef060b40d7ab1d2e884ac538d696be5fa14fa07f7f1af471a59e1a9703199809a50de83b4d11a55adb488fdd45261aef529e5361a35c2fefd262e645bec560dc8f4018cad5c9ee103171e328a9121c20201176140a671b53877c7a856bda81afd9ffa45f257022d4c8eb0ec3fe0ea81c0b90ad9ed8367552484294043d832a8d112f7170dd1f9f35fb129b7d33d04c8052be9a53f9547ca75bc789fcf326751bab7d67e287e3b7a6b52b8f4811262cfab312c4ef8002e2042f2443e0b6a40485381af7d4626d10777a6711c2beb7682c36288c6ca6d608e1a";
    const RSA_SPKI: &str = "30820122300d06092a864886f70d01010105000382010f003082010a0282010100d583fa9ba6c986b5619f70c66377fc7e31ac08553da63c8f04b8dd7a8a4de656b047f2064fa28706a42139f21b33a53b24f5bd27c8dea735b59313b4982b998deeafc9a469115c87c1a1a777f312fbda847352288c21e518ba2d47359dd3ba58f9ccf042dbe1ae6091415a9a4267dd8183bc8bee374b4fb275b8986ed29078d22e4e413179a3d34a55199d4f94bc4d928558bad35f5a25905e8550db3fd53aafc50792fe44b6d5e67d3fc5ec6e989646c2858877bca8aa2ad254a9b55ab7ec7f1aa8fb5d53387452e0ae80a1003f65207f62c53dae67a154fda58b50be9957d5eb24dc86850ed5c9057e620aae9fd8078301048c3a659bcfa180a7c9049a31110203010001";

    fn unhex(text: &str) -> heapless::Vec<u8, 1024> {
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn finds_the_ecdsa_public_key() {
        let cert = unhex(EC_CERT);
        let key = public_key(&cert).unwrap();
        assert_eq!(key.info, unhex(EC_SPKI).as_slice());
        // Uncompressed SEC1 point: 0x04 + X + Y.
        assert_eq!(key.key.len(), 65);
        assert_eq!(key.key[0], 0x04);
    }

    #[test]
    fn finds_the_rsa_public_key() {
        let cert = unhex(RSA_CERT);
        let key = public_key(&cert).unwrap();
        assert_eq!(key.info, unhex(RSA_SPKI).as_slice());
        // A PKCS#1 RSAPublicKey is a SEQUENCE.
        assert_eq!(key.key[0], 0x30);
    }

    #[test]
    fn compares_certificates_by_their_public_key() {
        let ec = unhex(EC_CERT);
        let rsa = unhex(RSA_CERT);
        assert!(same_public_key(&ec, &ec));
        assert!(!same_public_key(&ec, &rsa));
    }

    #[test]
    fn a_renewed_certificate_with_the_same_key_matches() {
        // Same key, different certificate bytes: flip the last byte of the
        // serial number. The certificate's own signature no longer matches,
        // which pinning doesn't look at.
        let ec = unhex(EC_CERT);
        let (_, certificate, _) = tlv(&ec).unwrap();
        let (_, tbs, _) = tlv(certificate).unwrap();
        let (_, _, after_version) = tlv(tbs).unwrap();
        let (_, serial, _) = tlv(after_version).unwrap();
        let serial_last = serial.as_ptr() as usize - ec.as_ptr() as usize + serial.len() - 1;

        let mut renewed = ec.clone();
        renewed[serial_last] ^= 0xFF;
        assert_ne!(ec, renewed);
        assert!(same_public_key(&ec, &renewed));
    }

    #[test]
    fn malformed_certificates_have_no_public_key() {
        assert_eq!(public_key(&[]), None);
        assert_eq!(public_key(&[0x30, 0x05, 0x30]), None);
        let cert = unhex(EC_CERT);
        for cut in [10, 40, 120, cert.len() - 1] {
            assert_eq!(public_key(&cert[..cut]), None, "cut at {cut}");
        }
        assert!(!same_public_key(&[1, 2, 3], &cert));
    }

    #[test]
    fn verify_message_is_spaces_context_zero_and_hash() {
        let hash = [0xAB; 32];
        let message = server_verify_message(&hash);
        assert_eq!(message.len(), 64 + 34 + 32);
        assert!(message[..64].iter().all(|b| *b == 0x20));
        assert_eq!(&message[64..97], b"TLS 1.3, server CertificateVerify");
        assert_eq!(message[97], 0);
        assert_eq!(&message[98..], &hash);
    }

    #[test]
    fn verify_message_fits_a_sha384_transcript() {
        let message = server_verify_message(&[0x11; 48]);
        assert_eq!(message.len(), 64 + 34 + 48);
    }
}
