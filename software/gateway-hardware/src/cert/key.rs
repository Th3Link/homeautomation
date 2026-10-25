//! The device's HTTPS key: one ECDSA P-256 key, made on the gateway from the
//! hardware RNG the first time it is needed, kept in the config flash and
//! never given out (no route, no console command, no log line shows it).
//!
//! The key lies in flash unencrypted; see "No secure boot, no flash
//! encryption" in `technical-debt.md`.

use super::store;
use crate::config::config;
use crate::console_log;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use gateway_core::x509::issue::{self, Signature};
use p256::ecdsa::signature::Signer;
use p256::ecdsa::SigningKey;

/// The public point, kept once known (it isn't secret, and deriving it
/// takes a scalar multiplication).
static POINT: Mutex<CriticalSectionRawMutex, Option<[u8; 65]>> = Mutex::new(None);

/// The private scalar, wiped when dropped.
struct Scalar([u8; 32]);

impl Drop for Scalar {
    fn drop(&mut self) {
        wipe(&mut self.0);
    }
}

/// Overwrites key material so it doesn't linger in RAM.
pub fn wipe(bytes: &mut [u8]) {
    for byte in bytes.iter_mut() {
        // SAFETY: a valid, aligned `&mut u8`.
        unsafe { core::ptr::write_volatile(byte, 0) };
    }
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
}

async fn load() -> Option<Scalar> {
    let mut scalar = Scalar([0; 32]);
    let len = config()
        .await
        .read_bytes(store::DEVICE_KEY, &mut scalar.0)
        .await?;
    (len == 32 && SigningKey::from_slice(&scalar.0).is_ok()).then_some(scalar)
}

/// Makes the key if there is none yet. Returns whether one was made.
pub async fn ensure() -> Result<bool, ()> {
    if load().await.is_some() {
        return Ok(false);
    }
    let rng = esp_hal::rng::Trng::try_new().map_err(|_| ())?;
    let mut scalar = Scalar([0; 32]);
    // A random 256-bit number is a valid scalar unless it is 0 or >= n,
    // which practically never happens; retry anyway.
    loop {
        rng.read(&mut scalar.0);
        if SigningKey::from_slice(&scalar.0).is_ok() {
            break;
        }
    }
    config()
        .await
        .set_bytes(store::DEVICE_KEY, &scalar.0)
        .await?;
    *POINT.lock().await = None;
    console_log!("https: made a new device key");
    Ok(true)
}

fn signing_key(scalar: &Scalar) -> Option<SigningKey> {
    SigningKey::from_slice(&scalar.0).ok()
}

/// The public key as an uncompressed SEC1 point.
pub async fn point() -> Option<[u8; 65]> {
    if let Some(point) = *POINT.lock().await {
        return Some(point);
    }
    let scalar = load().await?;
    let key = signing_key(&scalar)?;
    let encoded = key.verifying_key().to_sec1_point(false);
    let point: [u8; 65] = encoded.as_bytes().try_into().ok()?;
    *POINT.lock().await = Some(point);
    Some(point)
}

/// Signs `data` (ECDSA with SHA-256, deterministic per RFC 6979); returns
/// the DER signature.
pub async fn signer() -> Option<impl FnOnce(&[u8]) -> Option<Signature>> {
    let scalar = load().await?;
    Some(move |data: &[u8]| {
        let key = signing_key(&scalar)?;
        let signature: p256::ecdsa::Signature = key.sign(data);
        Signature::from_slice(signature.to_der().as_bytes()).ok()
    })
}

/// The key in the form MbedTLS loads, parsed straight into an MbedTLS key
/// (the DER copy is wiped).
pub async fn mbedtls_key() -> Option<mbedtls_rs::PrivateKey> {
    let scalar = load().await?;
    let point = point().await?;
    let mut der = [0u8; 128];
    let key = match issue::sec1_private_key(&mut der, &scalar.0, &point) {
        Ok(len) => mbedtls_rs::PrivateKey::new(mbedtls_rs::X509::DER(&der[..len]), None).ok(),
        Err(_) => None,
    };
    wipe(&mut der);
    key
}
