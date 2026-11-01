//! TLS 1.3 for the `mqtts://` broker connections, on top of
//! [`embedded-tls`](embedded_tls) (pure Rust, no heap for the connection).
//!
//! # Trusting a broker
//!
//! A broker is checked in one of three ways (ADR 0015, ADR 0020):
//!
//! - **Pinned key**, once one is pinned (below): for a broker with a
//!   self-signed certificate.
//! - **Certificate authority**, without a pinned key: the broker's chain
//!   must lead to one of the trusted CAs ([`crate::cert::roots`]: Let's
//!   Encrypt's built in, and those the user installs), name the host of the
//!   broker URI (SAN DNS name, or IP address for an address), be valid now
//!   (so the time must be known, from SNTP) and allow `serverAuth`. MbedTLS
//!   checks the chain ([`verify_chain`]); the host name is also sent as SNI.
//! - **Any certificate** with `mqtt-insecure on`.
//!
//! A broker on the LAN often has a self-signed certificate, so there's no
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
//! `mqtt-untrust` forgets the pinned certificate (and so goes back to the
//! certificate authorities); `mqtt-insecure on` instead accepts any
//! certificate (the traffic is encrypted, but a man in the middle is not
//! excluded).
//!
//! With a pinned key, certificate validity dates, the issuing authority and
//! the host name aren't checked: the pinned key takes the place of the rest.
//!
//! # Limits
//!
//! TLS 1.3 only (every current Mosquitto/OpenSSL speaks it), one cipher
//! suite (`TLS_AES_128_GCM_SHA256`), certificates with ECDSA P-256/P-384 or
//! RSA keys; a pinned certificate at most [`CERT_CAP`] bytes long, a chain
//! within one [`READ_RECORD_BYTES`] record.

use crate::cert::roots::Trusted;
use alloc::boxed::Box;
use core::ffi::CStr;
use core::fmt;
use core::marker::PhantomData;
use embassy_net::tcp::TcpSocket;
use embedded_tls::{
    Aes128GcmSha256, CertificateEntryRef, CertificateRef, CertificateVerifyRef, CryptoProvider,
    CryptoRngCore, NoVerify, SignatureScheme, TlsCipherSuite, TlsConfig, TlsConnection, TlsContext,
    TlsError, TlsVerifier,
};
use esp_hal::rng::{Trng, TrngSource};
use gateway_core::tls_pin;
use heapless::{String, Vec};
use mbedtls_rs::sys as mbedtls;
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
    /// Its chain has to lead to one of `roots` and name `host`; why it
    /// didn't is left in `problems` ([`ChainProblems`]).
    Authority {
        roots: &'a Trusted,
        host: &'a str,
        problems: &'a mut u32,
    },
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
    // RSA signatures are offered, or an RSA broker would end the handshake
    // right away.
    let config = TlsConfig::new().enable_rsa_signatures();
    let rng = Trng::try_new().map_err(|_| TlsError::InternalError)?;

    match trust {
        Trust::Pinned(certificate) => {
            // No server name: the pinned key is all that counts.
            let provider = KeyProvider {
                rng,
                verifier: KeyVerifier::new(Check::Pinned(certificate)),
            };
            connection.open(TlsContext::new(&config, provider)).await?;
        }
        Trust::Authority {
            roots,
            host,
            problems,
        } => {
            // The host name as SNI (an address isn't sent, RFC 6066), so
            // that a proxy in front of the broker can route by it.
            let config = if host.parse::<core::net::IpAddr>().is_ok() {
                config
            } else {
                config.with_server_name(host)
            };
            let provider = KeyProvider {
                rng,
                verifier: KeyVerifier::new(Check::Authority {
                    roots,
                    host,
                    problems,
                }),
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

/// Verifies the server's certificate and its handshake signature.
struct KeyProvider<'a> {
    rng: Trng,
    verifier: KeyVerifier<'a>,
}

impl CryptoProvider for KeyProvider<'_> {
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

/// What the server's certificate is checked against.
enum Check<'a> {
    /// The pinned certificate (DER): the same public key.
    Pinned(&'a [u8]),
    /// The trusted CAs and the host name ([`verify_chain`]).
    Authority {
        roots: &'a Trusted,
        host: &'a str,
        problems: &'a mut u32,
    },
}

/// Accepts a server whose certificate passes the [`Check`] and which proves
/// it owns the certificate's key by signing the handshake.
struct KeyVerifier<'a> {
    check: Check<'a>,
    /// The handshake hash up to the server's `Certificate` message.
    transcript: Option<<Aes128GcmSha256 as TlsCipherSuite>::Hash>,
    /// The server's public key, once its certificate was accepted.
    key: Vec<u8, KEY_CAP>,
}

impl<'a> KeyVerifier<'a> {
    fn new(check: Check<'a>) -> Self {
        Self {
            check,
            transcript: None,
            key: Vec::new(),
        }
    }
}

impl TlsVerifier<Aes128GcmSha256> for KeyVerifier<'_> {
    fn set_hostname_verification(&mut self, _hostname: &str) -> Result<(), TlsError> {
        // The host name is checked by `verify_chain`, where wanted.
        Ok(())
    }

    fn verify_certificate(
        &mut self,
        transcript: &<Aes128GcmSha256 as TlsCipherSuite>::Hash,
        cert: CertificateRef,
    ) -> Result<(), TlsError> {
        let Some(CertificateEntryRef::X509(leaf)) = cert.entries.first() else {
            return Err(TlsError::InvalidCertificate);
        };
        let presented = tls_pin::public_key(leaf).ok_or(TlsError::InvalidCertificate)?;
        match &mut self.check {
            Check::Pinned(pinned) => {
                let pinned = tls_pin::public_key(pinned).ok_or(TlsError::InvalidCertificate)?;
                if presented.info != pinned.info {
                    return Err(TlsError::InvalidCertificate);
                }
            }
            Check::Authority {
                roots,
                host,
                problems,
            } => {
                let chain = cert.entries.iter().filter_map(|entry| match entry {
                    CertificateEntryRef::X509(der) => Some(*der),
                    _ => None,
                });
                if let Err(found) = verify_chain(chain, roots.iter(), host) {
                    **problems = found;
                    return Err(TlsError::InvalidCertificate);
                }
            }
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

/// The `serverAuth` extended key usage (1.3.6.1.5.5.7.3.1), DER content.
const SERVER_AUTH: &[u8] = &[0x2B, 0x06, 0x01, 0x05, 0x05, 0x07, 0x03, 0x01];

/// Checks a server's certificate chain (DER, leaf first) with MbedTLS, as
/// its TLS client would: signatures up to one of `roots`, every certificate
/// valid at the current time (from SNTP: without it MbedTLS refuses the
/// dates), the leaf naming `host` (SAN DNS name, or SAN IP address for an
/// address) and allowed for `digitalSignature` and `serverAuth` (where it
/// says). Returns MbedTLS' `MBEDTLS_X509_BADCERT_*` flags on failure,
/// see [`ChainProblems`].
///
/// The certificates aren't copied, only MbedTLS' parsed view of them is on
/// the heap while this runs.
pub fn verify_chain<'a>(
    chain: impl Iterator<Item = &'a [u8]>,
    roots: impl Iterator<Item = &'a [u8]>,
    host: &str,
) -> Result<(), u32> {
    let mut name: String<256> = String::new();
    if name.push_str(host).is_err() || name.push('\0').is_err() {
        return Err(mbedtls::MBEDTLS_X509_BADCERT_CN_MISMATCH);
    }
    let name = CStr::from_bytes_with_nul(name.as_bytes())
        .map_err(|_| mbedtls::MBEDTLS_X509_BADCERT_CN_MISMATCH)?;

    let mut presented = Certificates::new();
    for der in chain {
        if !presented.add(der) {
            return Err(mbedtls::MBEDTLS_X509_BADCERT_OTHER);
        }
    }
    let mut trusted = Certificates::new();
    for der in roots {
        // An installed CA certificate was parsed before it was stored; one
        // that MbedTLS can't read is just not trusted.
        let _ = trusted.add(der);
    }

    let mut flags = 0u32;
    // SAFETY: both lists were set up by `Certificates` and outlive the
    // call; `name` is NUL-terminated; MbedTLS writes only `flags`.
    let result = unsafe {
        mbedtls::mbedtls_x509_crt_verify(
            presented.as_mut_ptr(),
            trusted.as_mut_ptr(),
            core::ptr::null_mut(),
            name.as_ptr(),
            &mut flags,
            None,
            core::ptr::null_mut(),
        )
    };
    if result != 0 {
        return Err(if flags == 0 {
            mbedtls::MBEDTLS_X509_BADCERT_OTHER
        } else {
            flags
        });
    }

    // What MbedTLS' own TLS client checks of the leaf besides the chain.
    // SAFETY: `presented` holds at least the leaf (the chain verified).
    let leaf = presented.as_mut_ptr();
    let usage = unsafe {
        mbedtls::mbedtls_x509_crt_check_key_usage(
            leaf,
            mbedtls::MBEDTLS_X509_KU_DIGITAL_SIGNATURE as _,
        )
    };
    if usage != 0 {
        return Err(mbedtls::MBEDTLS_X509_BADCERT_KEY_USAGE);
    }
    let extended = unsafe {
        mbedtls::mbedtls_x509_crt_check_extended_key_usage(
            leaf,
            SERVER_AUTH.as_ptr().cast(),
            SERVER_AUTH.len(),
        )
    };
    if extended != 0 {
        return Err(mbedtls::MBEDTLS_X509_BADCERT_EXT_KEY_USAGE);
    }
    Ok(())
}

/// A list of certificates parsed by MbedTLS without copying them, so they
/// have to outlive it.
struct Certificates<'a> {
    // Boxed: MbedTLS links to it from the next entry of the list.
    list: Box<mbedtls::mbedtls_x509_crt>,
    _der: PhantomData<&'a [u8]>,
}

impl<'a> Certificates<'a> {
    fn new() -> Self {
        // SAFETY: a plain C struct, all-zero is a valid value; `init` sets
        // it up as an empty list.
        let mut list = Box::new(unsafe { core::mem::zeroed::<mbedtls::mbedtls_x509_crt>() });
        unsafe { mbedtls::mbedtls_x509_crt_init(&mut *list) };
        Self {
            list,
            _der: PhantomData,
        }
    }

    /// Appends one certificate; false if MbedTLS can't parse it.
    fn add(&mut self, der: &'a [u8]) -> bool {
        // SAFETY: `der` outlives the list (the lifetime), which MbedTLS
        // only reads.
        unsafe {
            mbedtls::mbedtls_x509_crt_parse_der_nocopy(&mut *self.list, der.as_ptr(), der.len())
                == 0
        }
    }

    fn as_mut_ptr(&mut self) -> *mut mbedtls::mbedtls_x509_crt {
        &mut *self.list
    }
}

impl Drop for Certificates<'_> {
    fn drop(&mut self) {
        // SAFETY: set up by `init`; frees what MbedTLS allocated for it.
        unsafe { mbedtls::mbedtls_x509_crt_free(&mut *self.list) };
    }
}

/// Why a chain wasn't accepted, from [`verify_chain`]'s flags, for the log.
pub struct ChainProblems(pub u32);

impl fmt::Display for ChainProblems {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const NAMES: [(u32, &str); 10] = [
            (
                mbedtls::MBEDTLS_X509_BADCERT_NOT_TRUSTED,
                "not issued by a trusted CA",
            ),
            (
                mbedtls::MBEDTLS_X509_BADCERT_CN_MISMATCH,
                "not for this host name",
            ),
            (mbedtls::MBEDTLS_X509_BADCERT_EXPIRED, "expired"),
            (mbedtls::MBEDTLS_X509_BADCERT_FUTURE, "not yet valid"),
            (mbedtls::MBEDTLS_X509_BADCERT_REVOKED, "revoked"),
            (
                mbedtls::MBEDTLS_X509_BADCERT_KEY_USAGE,
                "key usage doesn't allow signing",
            ),
            (
                mbedtls::MBEDTLS_X509_BADCERT_EXT_KEY_USAGE,
                "not for TLS servers (extended key usage)",
            ),
            (mbedtls::MBEDTLS_X509_BADCERT_BAD_MD, "weak hash"),
            (mbedtls::MBEDTLS_X509_BADCERT_BAD_KEY, "weak key"),
            (mbedtls::MBEDTLS_X509_BADCERT_BAD_PK, "unsupported key type"),
        ];
        let mut first = true;
        for (flag, name) in NAMES {
            if self.0 & flag != 0 {
                if !first {
                    f.write_str(", ")?;
                }
                f.write_str(name)?;
                first = false;
            }
        }
        if first {
            write!(f, "rejected (0x{:x})", self.0)?;
        }
        Ok(())
    }
}
