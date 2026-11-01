//! The certificate authorities the gateway trusts (ADR 0020): Let's
//! Encrypt's roots, built in and on unless switched off, and the CA
//! certificates the user installs. The EST client checks its server against
//! them, and so does an `mqtts://` connection to a broker whose key isn't
//! pinned ([`crate::tls`]).

use super::store;
use crate::config::config;
use alloc::vec::Vec;
use gateway_core::x509::cert::split_concatenated;

/// A root certificate compiled into the firmware.
pub struct BuiltIn {
    pub name: &'static str,
    pub der: &'static [u8],
}

/// Let's Encrypt's roots (`certs/README.md` has where they come from and
/// their fingerprints). X1 and X2 are in every trust store; Root YE and
/// Root YR head the hierarchy Let's Encrypt issues from and are
/// cross-signed by X2 and X1, so they only let chains that end at them
/// verify.
pub const BUILT_IN: [BuiltIn; 4] = [
    BuiltIn {
        name: "ISRG Root X1",
        der: include_bytes!("../../certs/isrg-root-x1.der"),
    },
    BuiltIn {
        name: "ISRG Root X2",
        der: include_bytes!("../../certs/isrg-root-x2.der"),
    },
    BuiltIn {
        name: "ISRG Root YE",
        der: include_bytes!("../../certs/isrg-root-ye.der"),
    },
    BuiltIn {
        name: "ISRG Root YR",
        der: include_bytes!("../../certs/isrg-root-yr.der"),
    },
];

/// Whether the built-in roots are trusted (the default).
pub async fn built_in_enabled() -> bool {
    config()
        .await
        .get_bool(store::BUILT_IN_ROOTS)
        .await
        .unwrap_or(true)
}

/// The CA certificates trusted right now, read once for one check.
pub struct Trusted {
    built_in: bool,
    /// Concatenated DER, as stored.
    installed: Option<Vec<u8>>,
}

impl Trusted {
    pub async fn load() -> Self {
        Self {
            built_in: built_in_enabled().await,
            installed: store::TRUST_ANCHOR.read().await,
        }
    }

    /// Every trusted CA certificate (DER).
    pub fn iter(&self) -> impl Iterator<Item = &[u8]> {
        let built_in = BUILT_IN
            .iter()
            .filter(|_| self.built_in)
            .map(|root| root.der);
        let installed = self
            .installed
            .as_deref()
            .into_iter()
            .flat_map(split_concatenated);
        built_in.chain(installed)
    }

    pub fn is_empty(&self) -> bool {
        self.iter().next().is_none()
    }
}
