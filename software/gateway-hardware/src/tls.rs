//! TLS 1.3 for the `mqtts://` broker connections, on top of
//! [`embedded-tls`](embedded_tls) (pure Rust, no heap).
//!
//! # Trusting a broker
//!
//! A broker on the LAN usually has a self-signed certificate, so there's no
//! certificate authority to check it against. Instead the gateway *pins* the
//! certificate (trust on first use, but only when asked to):
//!
//! 1. `mqtt-trust` (or `mqtt2-trust`) makes the next connection to that
//!    broker a *capture*: the certificate the broker presents is taken
//!    without checking it and stored in flash ([`crate::config`]), and its
//!    SHA-256 fingerprint is logged for comparison with the broker's own.
//! 2. From then on, every connection to that broker must present a
//!    certificate with the same *public key* and prove, with the handshake
//!    signature, that it holds the private key — or it is refused. Pinning
//!    the key instead of the certificate keeps working when the certificate
//!    is renewed under the same key, and it makes no difference whether the
//!    certificate is self-signed or issued by a certificate authority.
//!
//! `mqtt-untrust` forgets the pinned certificate; `mqtt-insecure on`
//! instead accepts any certificate (the traffic is encrypted, but a man in
//! the middle is not excluded).
//!
//! Certificate validity dates, the issuing authority and the host name aren't
//! checked: the gateway has no clock, and the pinned key takes the place of
//! the rest.
//!
//! # Limits
//!
//! TLS 1.3 only (every current Mosquitto/OpenSSL speaks it), one cipher
//! suite (`TLS_AES_128_GCM_SHA256`), certificates with ECDSA P-256/P-384 or
//! RSA keys, at most [`CERT_CAP`] bytes long.

use embassy_net::tcp::TcpSocket;
use embedded_tls::{
    Aes128GcmSha256, CertificateEntryRef, CertificateRef, CertificateVerifyRef, CryptoProvider,
    CryptoRngCore, NoVerify, SignatureScheme, TlsCipherSuite, TlsConfig, TlsConnection, TlsContext,
    TlsError, TlsVerifier,
};
use esp_hal::rng::{Trng, TrngSource};
use gateway_core::tls_pin;
use heapless::Vec;
// embedded-tls 0.19 and rsa 0.9 are still on sha2 0.10 (digest 0.10): the
// handshake transcript and the RSA-PSS digests take the copy rsa re-exports.
use rsa::sha2 as handshake_sha2;
use sha2::{Digest, Sha256};

/// Largest server certificate (DER) that can be pinned or captured.
pub const CERT_CAP: usize = 2048;
/// Buffer for one received TLS record. Records can be up to 16 KiB; this is
/// enough for a certificate chain and any message this gateway subscribes
/// to, and keeps two simultaneous connections affordable.
pub const READ_RECORD_BYTES: usize = 8192;
/// Buffer for one record being sent (handshake messages and MQTT packets).
pub const WRITE_RECORD_BYTES: usize = 2560;

/// An open TLS connection over a TCP socket.
pub type Connection<'a> = TlsConnection<'a, TcpSocket<'a>, Aes128GcmSha256>;

/// What the server's certificate has to satisfy.
pub enum Trust<'a> {
    /// It has to verify against exactly this pinned certificate (DER).
    Pinned(&'a [u8]),
    /// Anything goes: encrypted, but not authenticated.
    AnyCertificate,
    /// Anything goes, and the certificate is copied into the given buffer.
    Capture(&'a mut Vec<u8, CERT_CAP>),
}

/// Switches on the hardware random number generator's true-random mode.
///
/// TLS keys must not be predictable, and the ESP32's generator only
/// produces real entropy while the radio or an ADC noise source feeds it;
/// the gateway has no radio, so this turns the ADC source on for good.
pub fn init_entropy(
    rng: esp_hal::peripherals::RNG<'static>,
    adc1: esp_hal::peripherals::ADC1<'static>,
) {
    // Dropping the source would switch the entropy off again.
    core::mem::forget(TrngSource::new(rng, adc1));
}

/// SHA-256 of a certificate (DER), as shown by `openssl x509 -fingerprint
/// -sha256` (that is, over the DER encoding).
pub fn fingerprint(certificate: &[u8]) -> [u8; 32] {
    Sha256::digest(certificate).into()
}

/// Performs the TLS handshake over `socket`. On success the connection is
/// ready for application data.
pub async fn open<'a>(
    socket: TcpSocket<'a>,
    read_buffer: &'a mut [u8],
    write_buffer: &'a mut [u8],
    trust: Trust<'_>,
) -> Result<Connection<'a>, TlsError> {
    let mut connection = Connection::new(socket, read_buffer, write_buffer);
    // No server name: neither SNI nor host name matching is wanted (see the
    // module docs). RSA signatures are offered, or an RSA broker would end
    // the handshake right away.
    let config = TlsConfig::new().enable_rsa_signatures();
    let rng = Trng::try_new().map_err(|_| TlsError::InternalError)?;

    match trust {
        Trust::Pinned(certificate) => {
            let provider = PinnedProvider {
                rng,
                verifier: PinnedVerifier {
                    pinned: certificate,
                    transcript: None,
                    key: Vec::new(),
                },
            };
            connection.open(TlsContext::new(&config, provider)).await?;
        }
        Trust::AnyCertificate => {
            let provider = AnyProvider {
                rng,
                verifier: NoVerify,
            };
            connection.open(TlsContext::new(&config, provider)).await?;
        }
        Trust::Capture(out) => {
            let provider = CaptureProvider {
                rng,
                verifier: CaptureVerifier { out },
            };
            connection.open(TlsContext::new(&config, provider)).await?;
        }
    }
    Ok(connection)
}

/// Verifies the server against a pinned certificate.
struct PinnedProvider<'a> {
    rng: Trng,
    verifier: PinnedVerifier<'a>,
}

impl CryptoProvider for PinnedProvider<'_> {
    type CipherSuite = Aes128GcmSha256;
    // No client certificate is ever offered, so no signature is made.
    type Signature = [u8; 0];

    fn rng(&mut self) -> impl CryptoRngCore {
        &mut self.rng
    }

    fn verifier(&mut self) -> Result<&mut impl TlsVerifier<Self::CipherSuite>, TlsError> {
        Ok(&mut self.verifier)
    }
}

/// Accepts any server certificate.
struct AnyProvider {
    rng: Trng,
    verifier: NoVerify,
}

impl CryptoProvider for AnyProvider {
    type CipherSuite = Aes128GcmSha256;
    type Signature = [u8; 0];

    fn rng(&mut self) -> impl CryptoRngCore {
        &mut self.rng
    }

    fn verifier(&mut self) -> Result<&mut impl TlsVerifier<Self::CipherSuite>, TlsError> {
        Ok(&mut self.verifier)
    }
}

/// Accepts any server certificate and keeps a copy of it.
struct CaptureProvider<'a> {
    rng: Trng,
    verifier: CaptureVerifier<'a>,
}

impl CryptoProvider for CaptureProvider<'_> {
    type CipherSuite = Aes128GcmSha256;
    type Signature = [u8; 0];

    fn rng(&mut self) -> impl CryptoRngCore {
        &mut self.rng
    }

    fn verifier(&mut self) -> Result<&mut impl TlsVerifier<Self::CipherSuite>, TlsError> {
        Ok(&mut self.verifier)
    }
}

struct CaptureVerifier<'a> {
    out: &'a mut Vec<u8, CERT_CAP>,
}

impl TlsVerifier<Aes128GcmSha256> for CaptureVerifier<'_> {
    fn set_hostname_verification(&mut self, _hostname: &str) -> Result<(), TlsError> {
        Ok(())
    }

    fn verify_certificate(
        &mut self,
        _transcript: &<Aes128GcmSha256 as TlsCipherSuite>::Hash,
        cert: CertificateRef,
    ) -> Result<(), TlsError> {
        // The first entry is the server's own certificate.
        let Some(CertificateEntryRef::X509(der)) = cert.entries.first() else {
            return Err(TlsError::InvalidCertificate);
        };
        self.out.clear();
        self.out
            .extend_from_slice(der)
            .map_err(|_| TlsError::InsufficientSpace)
    }

    fn verify_signature(&mut self, _verify: CertificateVerifyRef) -> Result<(), TlsError> {
        Ok(())
    }
}

/// The most bytes of a server's public key that can be kept for checking
/// the handshake signature (an ECDSA P-384 point is 97, an RSA-4096
/// `RSAPublicKey` 526).
const KEY_CAP: usize = 560;

/// Accepts a server whose certificate has the pinned public key and which
/// proves it owns that key by signing the handshake.
struct PinnedVerifier<'a> {
    /// The pinned certificate (DER).
    pinned: &'a [u8],
    /// The handshake hash up to the server's `Certificate` message.
    transcript: Option<<Aes128GcmSha256 as TlsCipherSuite>::Hash>,
    /// The server's public key, once its certificate was accepted.
    key: Vec<u8, KEY_CAP>,
}

impl TlsVerifier<Aes128GcmSha256> for PinnedVerifier<'_> {
    fn set_hostname_verification(&mut self, _hostname: &str) -> Result<(), TlsError> {
        Ok(())
    }

    fn verify_certificate(
        &mut self,
        transcript: &<Aes128GcmSha256 as TlsCipherSuite>::Hash,
        cert: CertificateRef,
    ) -> Result<(), TlsError> {
        let Some(CertificateEntryRef::X509(presented)) = cert.entries.first() else {
            return Err(TlsError::InvalidCertificate);
        };
        let presented = tls_pin::public_key(presented).ok_or(TlsError::InvalidCertificate)?;
        let pinned = tls_pin::public_key(self.pinned).ok_or(TlsError::InvalidCertificate)?;
        if presented.info != pinned.info {
            return Err(TlsError::InvalidCertificate);
        }
        self.key = Vec::from_slice(presented.key).map_err(|_| TlsError::InsufficientSpace)?;
        self.transcript = Some(transcript.clone());
        Ok(())
    }

    fn verify_signature(&mut self, verify: CertificateVerifyRef) -> Result<(), TlsError> {
        let transcript = self.transcript.take().ok_or(TlsError::InternalError)?;
        let hash = handshake_sha2::Digest::finalize(transcript);
        let message = tls_pin::server_verify_message(hash.as_slice());

        let verified = match verify.signature_scheme {
            SignatureScheme::EcdsaSecp256r1Sha256 => {
                use p256::ecdsa::{signature::Verifier, Signature, VerifyingKey};
                let key =
                    VerifyingKey::from_sec1_bytes(&self.key).map_err(|_| TlsError::DecodeError)?;
                let signature =
                    Signature::from_der(verify.signature).map_err(|_| TlsError::DecodeError)?;
                key.verify(&message, &signature).is_ok()
            }
            SignatureScheme::EcdsaSecp384r1Sha384 => {
                use p384::ecdsa::{signature::Verifier, Signature, VerifyingKey};
                let key =
                    VerifyingKey::from_sec1_bytes(&self.key).map_err(|_| TlsError::DecodeError)?;
                let signature =
                    Signature::from_der(verify.signature).map_err(|_| TlsError::DecodeError)?;
                key.verify(&message, &signature).is_ok()
            }
            SignatureScheme::RsaPssRsaeSha256 => {
                verify_rsa_pss::<handshake_sha2::Sha256>(&self.key, &message, verify.signature)?
            }
            SignatureScheme::RsaPssRsaeSha384 => {
                verify_rsa_pss::<handshake_sha2::Sha384>(&self.key, &message, verify.signature)?
            }
            _ => return Err(TlsError::InvalidSignatureScheme),
        };
        if verified {
            Ok(())
        } else {
            Err(TlsError::InvalidSignature)
        }
    }
}

/// Checks an RSA-PSS signature (TLS 1.3's only RSA scheme for handshake
/// signatures) made over `message` by the holder of the PKCS#1
/// `RSAPublicKey` in `key`.
fn verify_rsa_pss<D>(key: &[u8], message: &[u8], signature: &[u8]) -> Result<bool, TlsError>
where
    D: handshake_sha2::Digest + handshake_sha2::digest::FixedOutputReset,
{
    use rsa::pkcs1::DecodeRsaPublicKey;
    use rsa::pss::{Signature, VerifyingKey};
    use rsa::signature::Verifier;

    let key = rsa::RsaPublicKey::from_pkcs1_der(key).map_err(|_| TlsError::DecodeError)?;
    let key = VerifyingKey::<D>::new(key);
    let signature = Signature::try_from(signature).map_err(|_| TlsError::DecodeError)?;
    Ok(key.verify(message, &signature).is_ok())
}
