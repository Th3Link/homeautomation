//! Where the certificate manager keeps its things in the config flash (raw
//! keys 0x90-0xAF, clear of [`crate::config::Key`] and the MQTT pins).
//!
//! Certificates are larger than one config item (2 KiB), so a chain lives
//! in a [`Blob`]: chunks in one of two slots, plus a pointer item saying
//! which slot holds how many bytes. A new chain goes into the slot not in
//! use and becomes active with the one write of the pointer, so a power cut
//! leaves either the old chain or the new one, never a mix.

use crate::config::config;
use alloc::vec::Vec;

/// One config item's worth of a blob.
const CHUNK: usize = 2048;

pub const DEVICE_KEY: u8 = 0x90;
pub const MODE: u8 = 0x91;
pub const FQDN: u8 = 0x92;
pub const VALIDITY_YEARS: u8 = 0x93;
pub const EST_URL: u8 = 0x94;
pub const EST_AUTO_RENEW: u8 = 0x95;
pub const HTTPS_ENABLED: u8 = 0x96;
pub const NTP_SERVER: u8 = 0x97;
/// The private key of the EST device identity (DER, PKCS#8 or SEC1/PKCS#1).
pub const IDENTITY_KEY: u8 = 0x9B;
/// How the active certificate came to be, see [`super::Origin`].
pub const ORIGIN: u8 = 0x9C;

/// A value spread over up to `chunks` config items, in one of two slots.
pub struct Blob {
    pointer: u8,
    slots: [u8; 2],
    chunks: u8,
}

/// The web server's certificate chain (concatenated DER, leaf first).
pub const CERTIFICATE: Blob = Blob {
    pointer: 0x98,
    slots: [0xA0, 0xA3],
    chunks: 3,
};
/// The trust anchor(s) for the EST server (concatenated DER).
pub const TRUST_ANCHOR: Blob = Blob {
    pointer: 0x99,
    slots: [0xA6, 0xA9],
    chunks: 3,
};
/// The EST device identity's certificate chain (concatenated DER).
pub const IDENTITY: Blob = Blob {
    pointer: 0x9A,
    slots: [0xAC, 0xAE],
    chunks: 2,
};

const SLOT_B: u32 = 1 << 31;

impl Blob {
    pub const fn capacity(&self) -> usize {
        self.chunks as usize * CHUNK
    }

    /// The stored bytes, or `None` if nothing is stored.
    pub async fn read(&self) -> Option<Vec<u8>> {
        let mut store = config().await;
        let pointer = store.get_u32(self.pointer).await.unwrap_or(0);
        let len = (pointer & 0xFF_FFFF) as usize;
        if len == 0 || len > self.capacity() {
            return None;
        }
        let first = self.slots[usize::from(pointer & SLOT_B != 0)];
        let mut data = alloc::vec![0u8; len];
        for (i, chunk) in data.chunks_mut(CHUNK).enumerate() {
            let n = store.read_bytes(first + i as u8, chunk).await?;
            if n != chunk.len() {
                return None;
            }
        }
        Some(data)
    }

    /// Replaces the stored bytes (empty: forget them).
    pub async fn write(&self, data: &[u8]) -> Result<(), ()> {
        if data.len() > self.capacity() {
            return Err(());
        }
        let mut store = config().await;
        if data.is_empty() {
            return store.set_u32(self.pointer, 0).await;
        }
        let current = store.get_u32(self.pointer).await.unwrap_or(0);
        // Into the slot that isn't the active one.
        let use_b = current != 0 && current & SLOT_B == 0;
        let first = self.slots[usize::from(use_b)];
        for (i, chunk) in data.chunks(CHUNK).enumerate() {
            store.set_bytes(first + i as u8, chunk).await?;
        }
        let pointer = data.len() as u32 | if use_b { SLOT_B } else { 0 };
        store.set_u32(self.pointer, pointer).await
    }
}
