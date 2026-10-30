//! The certificate manager: everything about the web server's HTTPS
//! certificate in one place (ADR 0018). The web routes and the console only
//! call the functions here.
//!
//! - One device key ([`key`]), from which the CSR and the self-signed
//!   certificate are made.
//! - Three modes for the configured FQDN: [`Mode::SelfSigned`],
//!   [`Mode::Manual`] (CSR out, signed certificate in) and [`Mode::Est`]
//!   (enrollment and renewal over EST, [`est`]).
//! - A new certificate is checked completely (it parses, carries the device
//!   key, names the FQDN, isn't expired) before it replaces the active one,
//!   and the replacement is one flash write ([`store::Blob`]).
//! - Whatever happens, there is a usable certificate: without one the
//!   gateway makes a self-signed one ([`Origin::Fallback`]).

mod est;
mod key;
mod store;

use crate::config::{config, LoadOrInit};
use crate::console_log;
use crate::wall_clock;
use alloc::boxed::Box;
use alloc::string::String as HeapString;
use alloc::vec::Vec;
use core::fmt::Write as _;
use core::ops::Range;
use embassy_net::Stack;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant, Timer};
use gateway_core::est::Operation;
use gateway_core::x509::cert::{split_concatenated, Certificate};
use gateway_core::x509::check::{self, Problem};
use gateway_core::x509::issue::{self, SelfSigned};
use gateway_core::x509::pem::{self, Pem};
use gateway_core::x509::{pkcs7, time};
use heapless::String;

pub use est::EstError;
pub use key::wipe;

/// How the certificate is obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    SelfSigned,
    Manual,
    Est,
}

impl Mode {
    pub fn code(self) -> &'static str {
        match self {
            Mode::SelfSigned => "self_signed",
            Mode::Manual => "manual",
            Mode::Est => "est",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Some(match code {
            "self_signed" => Mode::SelfSigned,
            "manual" => Mode::Manual,
            "est" => Mode::Est,
            _ => return None,
        })
    }

    fn from_u8(value: u8) -> Self {
        match value {
            1 => Mode::Manual,
            2 => Mode::Est,
            _ => Mode::SelfSigned,
        }
    }

    fn to_u8(self) -> u8 {
        match self {
            Mode::SelfSigned => 0,
            Mode::Manual => 1,
            Mode::Est => 2,
        }
    }
}

/// Where the active certificate came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    SelfSigned,
    Manual,
    Est,
    /// Self-signed, because there was no usable certificate.
    Fallback,
}

impl Origin {
    pub fn code(self) -> &'static str {
        match self {
            Origin::SelfSigned => "self_signed",
            Origin::Manual => "manual",
            Origin::Est => "est",
            Origin::Fallback => "self_signed_fallback",
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            1 => Origin::Manual,
            2 => Origin::Est,
            3 => Origin::Fallback,
            _ => Origin::SelfSigned,
        }
    }

    fn to_u8(self) -> u8 {
        match self {
            Origin::SelfSigned => 0,
            Origin::Manual => 1,
            Origin::Est => 2,
            Origin::Fallback => 3,
        }
    }
}

/// Why a certificate operation failed; `code()` is what the API shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertError {
    /// The certificate (chain) was refused.
    Rejected(Problem),
    BadFqdn,
    BadUrl,
    BadValidity,
    /// Not PEM/DER, or too large to store.
    BadInput,
    /// No trust anchor is installed (EST needs one).
    NoTrustAnchor,
    /// The device key is missing or can't sign.
    Key,
    Storage,
    NoMemory,
}

impl CertError {
    pub fn code(self) -> &'static str {
        match self {
            CertError::Rejected(problem) => problem.code(),
            CertError::BadFqdn => "bad_fqdn",
            CertError::BadUrl => "bad_url",
            CertError::BadValidity => "bad_validity",
            CertError::BadInput => "bad_input",
            CertError::NoTrustAnchor => "no_trust_anchor",
            CertError::Key => "key",
            CertError::Storage => "storage",
            CertError::NoMemory => "no_memory",
        }
    }
}

pub const FQDN_CAP: usize = issue::FQDN_MAX;
pub const URL_CAP: usize = 200;
pub const NTP_CAP: usize = 64;
const DEFAULT_NTP: &str = "pool.ntp.org";
const DEFAULT_VALIDITY_YEARS: u8 = 10;
pub const MAX_VALIDITY_YEARS: u8 = 30;

/// The settings of the certificate manager.
#[derive(Debug, Clone)]
pub struct Settings {
    /// As configured; empty means "use the hostname".
    pub fqdn: String<FQDN_CAP>,
    pub mode: Mode,
    pub validity_years: u8,
    pub est_url: String<URL_CAP>,
    pub est_auto_renew: bool,
    /// As configured; empty means automatic ([`ntp_server`]).
    pub ntp_server: String<NTP_CAP>,
}

pub async fn settings() -> Settings {
    let mut store = config().await;
    Settings {
        fqdn: store.get_str(store::FQDN).await.unwrap_or_default(),
        mode: Mode::from_u8(store.get_u8(store::MODE).await.unwrap_or(0)),
        validity_years: store
            .get_u8(store::VALIDITY_YEARS)
            .await
            .filter(|y| (1..=MAX_VALIDITY_YEARS).contains(y))
            .unwrap_or(DEFAULT_VALIDITY_YEARS),
        est_url: store.get_str(store::EST_URL).await.unwrap_or_default(),
        est_auto_renew: store.get_bool(store::EST_AUTO_RENEW).await.unwrap_or(true),
        ntp_server: store.get_str(store::NTP_SERVER).await.unwrap_or_default(),
    }
}

/// The FQDN certificates are made for: the configured one, else the
/// hostname (a single DNS label, which is a valid name too).
pub async fn effective_fqdn() -> String<FQDN_CAP> {
    let fqdn = settings().await.fqdn;
    if !fqdn.is_empty() {
        return fqdn;
    }
    let hostname = config().await.load_or_init().await.hostname;
    let mut fqdn = String::new();
    let _ = fqdn.push_str(if issue::valid_fqdn(&hostname) {
        &hostname
    } else {
        "gateway"
    });
    fqdn
}

/// The NTP server SNTP asks: the configured one, else the one DHCPv6
/// named, else pool.ntp.org.
pub async fn ntp_server() -> String<NTP_CAP> {
    let configured = settings().await.ntp_server;
    if !configured.is_empty() {
        return configured;
    }
    crate::dhcpv6::ntp_server().unwrap_or_else(|| String::try_from(DEFAULT_NTP).unwrap_or_default())
}

/// Is HTTPS on? (`web-https off` on the console turns the web server back
/// to plain HTTP on port 80, the way out if TLS ever doesn't work.)
pub async fn https_enabled() -> bool {
    config()
        .await
        .get_bool(store::HTTPS_ENABLED)
        .await
        .unwrap_or(true)
}

pub async fn set_https_enabled(on: bool) -> Result<(), CertError> {
    config()
        .await
        .set_bool(store::HTTPS_ENABLED, on)
        .await
        .map_err(|_| CertError::Storage)
}

/// The changes [`update_settings`] takes; `None` leaves a value as it is.
#[derive(Debug, Default)]
pub struct SettingsUpdate<'a> {
    pub fqdn: Option<&'a str>,
    pub mode: Option<Mode>,
    pub validity_years: Option<u8>,
    pub est_url: Option<&'a str>,
    pub est_auto_renew: Option<bool>,
    pub ntp_server: Option<&'a str>,
}

/// Changes the settings and acts on them: a new FQDN or the self-signed mode
/// gets a fresh self-signed certificate right away (in that mode), EST mode
/// starts an enrollment.
pub async fn update_settings(update: SettingsUpdate<'_>) -> Result<(), CertError> {
    // Check everything before changing anything.
    if let Some(fqdn) = update.fqdn {
        if !fqdn.is_empty() && !issue::valid_fqdn(fqdn) {
            return Err(CertError::BadFqdn);
        }
    }
    if let Some(url) = update.est_url {
        if !url.is_empty() && (gateway_core::est::parse_url(url).is_none() || url.len() > URL_CAP) {
            return Err(CertError::BadUrl);
        }
    }
    if let Some(years) = update.validity_years {
        if !(1..=MAX_VALIDITY_YEARS).contains(&years) {
            return Err(CertError::BadValidity);
        }
    }
    if let Some(server) = update.ntp_server {
        if server.len() > NTP_CAP {
            return Err(CertError::BadInput);
        }
    }

    let before = settings().await;
    {
        let mut store = config().await;
        let failed = |_| CertError::Storage;
        if let Some(fqdn) = update.fqdn {
            store.set_str(store::FQDN, fqdn).await.map_err(failed)?;
        }
        if let Some(mode) = update.mode {
            store
                .set_u8(store::MODE, mode.to_u8())
                .await
                .map_err(failed)?;
        }
        if let Some(years) = update.validity_years {
            store
                .set_u8(store::VALIDITY_YEARS, years)
                .await
                .map_err(failed)?;
        }
        if let Some(url) = update.est_url {
            store.set_str(store::EST_URL, url).await.map_err(failed)?;
        }
        if let Some(auto) = update.est_auto_renew {
            store
                .set_bool(store::EST_AUTO_RENEW, auto)
                .await
                .map_err(failed)?;
        }
        if let Some(server) = update.ntp_server {
            store
                .set_str(store::NTP_SERVER, server)
                .await
                .map_err(failed)?;
        }
    }
    let after = settings().await;

    let fqdn_changed = before.fqdn != after.fqdn;
    let mode_changed = before.mode != after.mode;
    if after.mode == Mode::SelfSigned && (fqdn_changed || mode_changed) {
        generate_self_signed(None).await?;
    }
    if after.mode == Mode::Est {
        let mut est = EST.lock().await;
        est.failures = 0;
        est.next_attempt = None;
        drop(est);
        WAKE.signal(());
    }
    Ok(())
}

/// Makes a new self-signed certificate for the FQDN with the device key and
/// activates it. `years` overrides the configured validity.
pub async fn generate_self_signed(years: Option<u8>) -> Result<(), CertError> {
    let years = match years {
        Some(years) if (1..=MAX_VALIDITY_YEARS).contains(&years) => years,
        Some(_) => return Err(CertError::BadValidity),
        None => settings().await.validity_years,
    };
    make_self_signed(years, Origin::SelfSigned).await
}

async fn make_self_signed(years: u8, origin: Origin) -> Result<(), CertError> {
    let fqdn = effective_fqdn().await;
    let point = key::point().await.ok_or(CertError::Key)?;
    let sign = key::signer().await.ok_or(CertError::Key)?;
    // A day back, so a client whose clock is a little behind accepts it.
    let not_before = wall_clock::now().unwrap_or_else(wall_clock::build_time) - time::DAY;
    let not_after = not_before + i64::from(years) * 365 * time::DAY + time::DAY;
    let mut serial = [0u8; 16];
    esp_hal::rng::Trng::try_new()
        .map_err(|_| CertError::Key)?
        .read(&mut serial);

    let mut der = alloc::vec![0u8; 1024];
    let len = issue::self_signed(
        &mut der,
        &SelfSigned {
            fqdn: &fqdn,
            serial,
            not_before,
            not_after,
        },
        &point,
        sign,
    )
    .map_err(|e| match e {
        issue::IssueError::BadFqdn => CertError::BadFqdn,
        _ => CertError::Key,
    })?;
    der.truncate(len);
    activate(&der, origin).await?;
    console_log!("https: self-signed certificate for {fqdn}, {years} years");
    Ok(())
}

/// Writes `chain` (concatenated DER, leaf first) as the active certificate.
async fn activate(chain: &[u8], origin: Origin) -> Result<(), CertError> {
    if chain.len() > store::CERTIFICATE.capacity() {
        return Err(CertError::Rejected(Problem::TooLong));
    }
    store::CERTIFICATE
        .write(chain)
        .await
        .map_err(|_| CertError::Storage)?;
    config()
        .await
        .set_u8(store::ORIGIN, origin.to_u8())
        .await
        .map_err(|_| CertError::Storage)
}

/// The most certificates an upload may hold (a chain, or a CA bundle as
/// trust anchor).
const UPLOAD_CERTS: usize = 8;

/// The certificates of an upload (PEM with one or more blocks, or DER, one
/// or more concatenated), decoded where they are: their places in `data`.
/// No copy, so that an upload fits the heap next to a TLS session.
fn decode_certificates(
    data: &mut [u8],
) -> Result<heapless::Vec<Range<usize>, UPLOAD_CERTS>, CertError> {
    let mut certs = heapless::Vec::new();
    if pem::is_pem(data) {
        let ends = pem::decode_in_place::<UPLOAD_CERTS>(data, "CERTIFICATE")
            .map_err(|_| CertError::BadInput)?;
        let mut start = 0;
        for end in ends {
            // As many as `ends` holds.
            let _ = certs.push(start..end);
            start = end;
        }
    } else {
        let base = data.as_ptr() as usize;
        for der in split_concatenated(data) {
            let start = der.as_ptr() as usize - base;
            certs
                .push(start..start + der.len())
                .map_err(|_| CertError::Rejected(Problem::TooLong))?;
        }
    }
    if certs.is_empty() {
        return Err(CertError::BadInput);
    }
    Ok(certs)
}

/// `parts` one after the other, in a buffer allocated without panicking.
fn concatenated<'a>(parts: impl Iterator<Item = &'a [u8]> + Clone) -> Result<Vec<u8>, CertError> {
    let mut out = Vec::new();
    out.try_reserve_exact(parts.clone().map(<[u8]>::len).sum())
        .map_err(|_| CertError::NoMemory)?;
    for part in parts {
        out.extend_from_slice(part);
    }
    Ok(out)
}

/// Checks a chain for the device key, the FQDN and the time, orders it leaf
/// first and returns it concatenated, ready to store.
async fn validated_chain<'a>(certs: impl Iterator<Item = &'a [u8]>) -> Result<Vec<u8>, CertError> {
    let point = key::point().await.ok_or(CertError::Key)?;
    let fqdn = effective_fqdn().await;
    let chain = check::order_chain(certs, &point).map_err(CertError::Rejected)?;
    check::check_leaf(&chain[0], &point, &fqdn, wall_clock::clock())
        .map_err(CertError::Rejected)?;
    concatenated(chain.iter().map(|cert| cert.der))
}

/// Installs an uploaded certificate (chain), PEM or DER, after checking it,
/// and switches to the manual mode.
pub async fn install_certificate(data: &mut [u8]) -> Result<(), CertError> {
    let certs = decode_certificates(data)?;
    let chain = validated_chain(certs.iter().map(|range| &data[range.clone()])).await?;
    activate(&chain, Origin::Manual).await?;
    // An uploaded certificate is the manual mode's, whatever the mode was:
    // EST or a new FQDN in self-signed mode would replace it otherwise.
    config()
        .await
        .set_u8(store::MODE, Mode::Manual.to_u8())
        .await
        .map_err(|_| CertError::Storage)?;
    console_log!("https: certificate installed");
    Ok(())
}

/// The CSR for the device key and the FQDN, as PEM.
pub async fn csr_pem() -> Result<HeapString, CertError> {
    let fqdn = effective_fqdn().await;
    let der = csr_der(&fqdn).await?;
    let mut text = HeapString::new();
    write!(
        text,
        "{}",
        Pem {
            label: "CERTIFICATE REQUEST",
            der: &der
        }
    )
    .map_err(|_| CertError::NoMemory)?;
    Ok(text)
}

async fn csr_der(fqdn: &str) -> Result<Vec<u8>, CertError> {
    let point = key::point().await.ok_or(CertError::Key)?;
    let sign = key::signer().await.ok_or(CertError::Key)?;
    let mut der = alloc::vec![0u8; 512];
    let len = issue::csr(&mut der, fqdn, &point, sign).map_err(|e| match e {
        issue::IssueError::BadFqdn => CertError::BadFqdn,
        _ => CertError::Key,
    })?;
    der.truncate(len);
    Ok(der)
}

/// Replaces the trust anchor (PEM or DER, one or more CA certificates);
/// empty data removes it.
pub async fn set_trust_anchor(data: &mut [u8]) -> Result<(), CertError> {
    if data.is_empty() {
        return store::TRUST_ANCHOR
            .write(&[])
            .await
            .map_err(|_| CertError::Storage);
    }
    let certs = decode_certificates(data)?;
    let certs = certs.iter().map(|range| &data[range.clone()]);
    for der in certs.clone() {
        Certificate::parse(der).ok_or(CertError::Rejected(Problem::Unparsable))?;
    }
    let all = concatenated(certs)?;
    if all.len() > store::TRUST_ANCHOR.capacity() {
        return Err(CertError::Rejected(Problem::TooLong));
    }
    store::TRUST_ANCHOR
        .write(&all)
        .await
        .map_err(|_| CertError::Storage)?;
    // EST may have been waiting for exactly this: try again now.
    let mut est = EST.lock().await;
    est.failures = 0;
    est.next_attempt = None;
    drop(est);
    WAKE.signal(());
    Ok(())
}

/// Installs the EST device identity: its certificate (chain) and private key,
/// both PEM in `data`. Empty data removes it. The key can't be read back.
pub async fn set_est_identity(data: &mut [u8]) -> Result<(), CertError> {
    if data.is_empty() {
        store::IDENTITY
            .write(&[])
            .await
            .map_err(|_| CertError::Storage)?;
        return config()
            .await
            .set_bytes(store::IDENTITY_KEY, &[])
            .await
            .map_err(|_| CertError::Storage);
    }
    // The key first: decoding the certificates overwrites `data`.
    let mut key_der = {
        let text = core::str::from_utf8(data).map_err(|_| CertError::BadInput)?;
        let (_, body) = pem::blocks(text)
            .find(|(label, _)| label.ends_with("PRIVATE KEY"))
            .ok_or(CertError::BadInput)?;
        let mut der = Vec::new();
        der.try_reserve_exact(body.len() * 3 / 4 + 3)
            .map_err(|_| CertError::NoMemory)?;
        der.resize(body.len() * 3 / 4 + 3, 0);
        let len = pem::decode(body.as_bytes(), &mut der).map_err(|_| CertError::BadInput)?;
        der.truncate(len);
        der
    };
    let chain = match decode_certificates(data).and_then(|certs| {
        let certs = certs.iter().map(|range| &data[range.clone()]);
        for der in certs.clone() {
            Certificate::parse(der).ok_or(CertError::Rejected(Problem::Unparsable))?;
        }
        concatenated(certs)
    }) {
        Ok(chain) => chain,
        Err(e) => {
            key::wipe(&mut key_der);
            return Err(e);
        }
    };
    // MbedTLS must be able to use it.
    let usable = mbedtls_rs::PrivateKey::new(mbedtls_rs::X509::DER(&key_der), None).is_ok();
    let result = if !usable || key_der.len() > 2048 {
        Err(CertError::BadInput)
    } else if chain.len() > store::IDENTITY.capacity() {
        Err(CertError::Rejected(Problem::TooLong))
    } else {
        let stored = config()
            .await
            .set_bytes(store::IDENTITY_KEY, &key_der)
            .await
            .map_err(|_| CertError::Storage);
        match stored {
            Ok(()) => store::IDENTITY
                .write(&chain)
                .await
                .map_err(|_| CertError::Storage),
            Err(e) => Err(e),
        }
    };
    key::wipe(&mut key_der);
    result
}

/// Asks for an EST enrollment now (in EST mode).
pub async fn request_enrollment() {
    let mut est = EST.lock().await;
    est.requested = true;
    est.next_attempt = None;
    drop(est);
    WAKE.signal(());
}

/// The time just became known: re-check the active certificate, maybe
/// renew.
pub fn time_known() {
    WAKE.signal(());
}

/// Facts about a certificate, for the status shown in the UI.
#[derive(Debug, Clone)]
pub struct CertInfo {
    pub subject: String<128>,
    pub issuer: String<128>,
    pub not_before: i64,
    pub not_after: i64,
    pub dns_names: heapless::Vec<String<FQDN_CAP>, 4>,
    pub fingerprint: [u8; 32],
}

fn info(cert: &Certificate<'_>) -> CertInfo {
    let mut subject = String::new();
    let _ = write!(
        subject,
        "{}",
        gateway_core::x509::cert::DisplayName(cert.subject)
    );
    let mut issuer = String::new();
    let _ = write!(
        issuer,
        "{}",
        gateway_core::x509::cert::DisplayName(cert.issuer)
    );
    let mut dns_names = heapless::Vec::new();
    for name in cert.dns_names() {
        if let Ok(name) = String::try_from(name) {
            let _ = dns_names.push(name);
        }
    }
    CertInfo {
        subject,
        issuer,
        not_before: cert.not_before,
        not_after: cert.not_after,
        dns_names,
        fingerprint: crate::tls::fingerprint(cert.der),
    }
}

/// The certificate manager's state, for the UI.
#[derive(Debug, Clone)]
pub struct Status {
    pub settings: Settings,
    pub effective_fqdn: String<FQDN_CAP>,
    pub https_enabled: bool,
    pub time_known: bool,
    pub certificate: Option<(CertInfo, check::Status, Origin)>,
    pub chain_length: usize,
    pub trust_anchor: Option<CertInfo>,
    pub trust_anchor_count: usize,
    pub est_identity: Option<CertInfo>,
    pub est: EstState,
}

/// The status, on the heap: it is ~5 KB, and a handler holding it by value
/// makes every web request's future, and the main stack frames that build
/// and poll those, that much larger (the router keeps room for its largest
/// handler).
pub async fn status() -> Box<Status> {
    crate::heap::boxed(|| async {
        let settings = settings().await;
        let fqdn = effective_fqdn().await;
        let clock = wall_clock::clock();

        let chain = store::CERTIFICATE.read().await;
        let origin = Origin::from_u8(config().await.get_u8(store::ORIGIN).await.unwrap_or(0));
        let (certificate, chain_length) = match chain.as_deref() {
            Some(chain) => (
                check::stored_leaf(chain)
                    .map(|leaf| (info(&leaf), check::status(&leaf, &fqdn, clock), origin)),
                split_concatenated(chain).count(),
            ),
            None => (None, 0),
        };
        let anchors = store::TRUST_ANCHOR.read().await;
        let (trust_anchor, trust_anchor_count) = match anchors.as_deref() {
            Some(anchors) => (
                check::stored_leaf(anchors).map(|c| info(&c)),
                split_concatenated(anchors).count(),
            ),
            None => (None, 0),
        };
        let identity = store::IDENTITY.read().await;
        let est_identity = identity
            .as_deref()
            .and_then(check::stored_leaf)
            .map(|c| info(&c));

        Box::new(Status {
            settings,
            effective_fqdn: fqdn,
            https_enabled: https_enabled().await,
            time_known: clock.known().is_some(),
            certificate,
            chain_length,
            trust_anchor,
            trust_anchor_count,
            est_identity,
            est: EST.lock().await.clone(),
        })
    })
    .await
}

/// What the web server presents: the active chain and the device key, as
/// MbedTLS objects.
pub async fn server_credentials() -> Option<mbedtls_rs::Credentials<'static>> {
    let chain = store::CERTIFICATE.read().await?;
    let certificate = mbedtls_certificate(&chain)?;
    drop(chain);
    let private_key = key::mbedtls_key().await?;
    Some(mbedtls_rs::Credentials {
        certificate,
        private_key,
    })
}

/// Concatenated DER certificates as one MbedTLS certificate chain (MbedTLS
/// reads a chain in one go only from PEM).
fn mbedtls_certificate(chain: &[u8]) -> Option<mbedtls_rs::Certificate<'static>> {
    let mut text = HeapString::new();
    for der in split_concatenated(chain) {
        write!(
            text,
            "{}",
            Pem {
                label: "CERTIFICATE",
                der
            }
        )
        .ok()?;
    }
    text.push('\0');
    let text = core::ffi::CStr::from_bytes_with_nul(text.as_bytes()).ok()?;
    mbedtls_rs::Certificate::new(mbedtls_rs::X509::PEM(text)).ok()
}

/// Makes sure there is a key and a usable certificate; at boot, before the
/// web server starts.
pub async fn init() {
    if key::ensure().await.is_err() {
        console_log!("https: could not make the device key");
        return;
    }
    ensure_usable().await;
}

/// Replaces the active certificate by a self-signed one if it is missing,
/// broken, not for the device key or expired (as far as can be told).
async fn ensure_usable() {
    let Some(point) = key::point().await else {
        return;
    };
    let usable = match store::CERTIFICATE.read().await {
        Some(chain) => match check::stored_leaf(&chain) {
            // Only an expired certificate is replaced: one that isn't valid
            // yet (uploaded while the time was unknown) becomes valid by
            // itself, and replacing it would lose it.
            Some(leaf) => {
                leaf.public_key == point
                    && check::check_dates(&leaf, wall_clock::clock()) != Err(Problem::Expired)
            }
            None => false,
        },
        None => false,
    };
    if !usable {
        console_log!("https: no usable certificate, making a self-signed one");
        let years = settings().await.validity_years;
        if make_self_signed(years, Origin::Fallback).await.is_err() {
            console_log!("https: could not make a certificate");
        }
    }
}

/// What the EST side has been up to.
#[derive(Debug, Clone, Default)]
pub struct EstState {
    /// Unix time of the last attempt (0: none, or the time wasn't known).
    pub last_attempt: i64,
    /// `ok`, or the error of the last attempt.
    pub last_result: Option<&'static str>,
    /// The HTTP status of a refusal.
    pub last_status: Option<u16>,
    pub failures: u32,
    /// When the next attempt is due, in seconds from now (for the UI).
    pub next_attempt: Option<Instant>,
    requested: bool,
}

static EST: Mutex<CriticalSectionRawMutex, EstState> = Mutex::new(EstState {
    last_attempt: 0,
    last_result: None,
    last_status: None,
    failures: 0,
    next_attempt: None,
    requested: false,
});

static WAKE: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// How often the manager looks at the certificate without being woken.
const CHECK_EVERY: Duration = Duration::from_secs(3600);

/// Watches the certificate: falls back when it expires and, in EST mode,
/// enrolls and renews.
#[embassy_executor::task]
pub async fn manager_task(stack: Stack<'static>) {
    loop {
        let next = EST.lock().await.next_attempt;
        let wait = match next {
            Some(at) if at > Instant::now() => (at - Instant::now()).min(CHECK_EVERY),
            Some(_) => Duration::from_millis(0),
            None => CHECK_EVERY,
        };
        let _ = embassy_futures::select::select(WAKE.wait(), Timer::after(wait)).await;

        // On the heap: the checks hold certificates and settings, which
        // the task's static storage would otherwise keep room for always.
        crate::heap::boxed(|| async {
            ensure_usable().await;
            if settings().await.mode == Mode::Est {
                est_step(stack).await;
            }
        })
        .await;
    }
}

/// One look at whether EST has to do something, and doing it.
async fn est_step(stack: Stack<'static>) {
    let settings = settings().await;
    if settings.est_url.is_empty() {
        return;
    }
    let Some(now) = wall_clock::now() else {
        // EST verifies the server's certificate dates; wait for the clock.
        EST.lock().await.last_result = Some("waiting_for_time");
        return;
    };
    {
        let est = EST.lock().await;
        if est.next_attempt.is_some_and(|at| at > Instant::now()) && !est.requested {
            return;
        }
    }

    let fqdn = effective_fqdn().await;
    let origin = Origin::from_u8(config().await.get_u8(store::ORIGIN).await.unwrap_or(0));
    let (ours, due) = {
        let chain = store::CERTIFICATE.read().await;
        let current = chain.as_deref().and_then(check::stored_leaf);
        (
            origin == Origin::Est && current.is_some_and(|leaf| check::covers(&leaf, &fqdn)),
            current.is_some_and(|leaf| check::renewal_due(leaf.not_before, leaf.not_after, now)),
        )
    };
    let requested = EST.lock().await.requested;

    // Not (yet) an EST certificate for this FQDN: enroll. Otherwise renew
    // when due (if automatic renewal is on) or when asked to.
    let operation = if !ours {
        Operation::Enroll
    } else if requested || (due && settings.est_auto_renew) {
        Operation::Reenroll
    } else {
        return;
    };

    let result = enroll(stack, &settings.est_url, &fqdn, operation, ours).await;
    let mut est = EST.lock().await;
    est.requested = false;
    est.last_attempt = now;
    match result {
        Ok(()) => {
            est.last_result = Some("ok");
            est.last_status = None;
            est.failures = 0;
            est.next_attempt = None;
        }
        Err(EnrollError::Est(EstError::Pending(seconds))) => {
            est.last_result = Some("pending");
            est.last_status = Some(202);
            est.next_attempt =
                Some(Instant::now() + Duration::from_secs(u64::from(seconds.max(60))));
        }
        Err(error) => {
            est.failures += 1;
            est.last_result = Some(error.code());
            est.last_status = match error {
                EnrollError::Est(EstError::Refused(status)) => Some(status),
                _ => None,
            };
            let wait = check::backoff(est.failures);
            est.next_attempt = Some(Instant::now() + Duration::from_secs(u64::from(wait)));
            console_log!(
                "https: EST {} failed ({}), next try in {} s",
                operation.path(),
                error.code(),
                wait
            );
        }
    }
}

enum EnrollError {
    Cert(CertError),
    Est(EstError),
}

impl EnrollError {
    fn code(&self) -> &'static str {
        match self {
            EnrollError::Cert(e) => e.code(),
            EnrollError::Est(e) => e.code(),
        }
    }
}

async fn enroll(
    stack: Stack<'static>,
    url: &str,
    fqdn: &str,
    operation: Operation,
    renewing: bool,
) -> Result<(), EnrollError> {
    let anchors = store::TRUST_ANCHOR
        .read()
        .await
        .ok_or(EnrollError::Cert(CertError::NoTrustAnchor))?;
    let mut trust = HeapString::new();
    for der in split_concatenated(&anchors) {
        write!(
            trust,
            "{}",
            Pem {
                label: "CERTIFICATE",
                der
            }
        )
        .map_err(|_| EnrollError::Cert(CertError::NoMemory))?;
    }
    trust.push('\0');
    drop(anchors);
    let trust = core::ffi::CStr::from_bytes_with_nul(trust.as_bytes())
        .map_err(|_| EnrollError::Cert(CertError::NoMemory))?;

    let csr = csr_der(fqdn).await.map_err(EnrollError::Cert)?;

    // Renewal authenticates with the certificate being renewed (RFC 7030
    // 4.2.2); a first enrollment with the device identity, if there is one.
    let client = if renewing {
        server_credentials().await
    } else {
        identity_credentials().await
    };

    let _slot = crate::https::slot().await;
    let p7 = est::exchange(stack, url, operation, &csr, trust, client)
        .await
        .map_err(EnrollError::Est)?;
    let certs = pkcs7::certificates(&p7).ok_or(EnrollError::Est(EstError::Malformed))?;
    let chain = validated_chain(certs).await.map_err(EnrollError::Cert)?;
    activate(&chain, Origin::Est)
        .await
        .map_err(EnrollError::Cert)?;
    console_log!(
        "https: EST {} for {fqdn}: certificate installed",
        operation.path()
    );
    Ok(())
}

async fn identity_credentials() -> Option<mbedtls_rs::Credentials<'static>> {
    let chain = store::IDENTITY.read().await?;
    let certificate = mbedtls_certificate(&chain)?;
    let mut der = alloc::vec![0u8; 2048];
    let len = config()
        .await
        .read_bytes(store::IDENTITY_KEY, &mut der)
        .await
        .filter(|n| *n > 0);
    let key = len
        .and_then(|len| mbedtls_rs::PrivateKey::new(mbedtls_rs::X509::DER(&der[..len]), None).ok());
    key::wipe(&mut der);
    Some(mbedtls_rs::Credentials {
        certificate,
        private_key: key?,
    })
}
