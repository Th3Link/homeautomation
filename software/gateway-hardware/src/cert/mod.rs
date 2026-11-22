//! The certificate manager: everything about the web server's HTTPS
//! certificate in one place (ADR 0018). The web routes and the console only
//! call the functions here.
//!
//! - One device key for both the CSR and the self-signed certificate.
//! - Three modes for the configured FQDN: [`Mode::SelfSigned`],
//!   [`Mode::Manual`] (CSR out, signed certificate in) and [`Mode::Est`]
//!   (enrollment and renewal over EST).
//! - A new certificate is checked completely (it parses, carries the device
//!   key, names the FQDN, isn't expired) before it replaces the active one,
//!   and the replacement is one flash write.
//! - Whatever happens, there is a usable certificate: without one the
//!   gateway makes a self-signed one ([`Origin::Fallback`]).

mod est;
mod key;
pub mod roots;
mod store;

use crate::config::{config, Key};
use crate::console_log;
use crate::wall_clock;
use core::fmt::Write as _;
use core::ops::Range;
use embassy_net::Stack;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::{MappedMutexGuard, Mutex, MutexGuard};
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
pub use key::{wipe, Sensitive};

/// One config item holds the DER private key for an EST client identity.
pub const IDENTITY_KEY_CAP: usize = 2048;
pub(crate) const UPLOAD_MAX: usize = 10 * 1024;
const _: () = assert!(UPLOAD_MAX + store::CERTIFICATE.capacity() <= crate::https::SCRATCH_SIZE);
const _: () = assert!(UPLOAD_MAX + IDENTITY_KEY_CAP <= crate::https::SCRATCH_SIZE);

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
    /// Whether the built-in root certificates are trusted ([`roots`]).
    pub built_in_roots: bool,
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
        built_in_roots: store.get_bool(store::BUILT_IN_ROOTS).await.unwrap_or(true),
    }
}

/// The FQDN certificates are made for: the configured one, else the
/// hostname (a single DNS label, which is a valid name too).
pub async fn effective_fqdn() -> String<FQDN_CAP> {
    let mut store = config().await;
    let fqdn: String<FQDN_CAP> = store.get_str(store::FQDN).await.unwrap_or_default();
    if !fqdn.is_empty() {
        return fqdn;
    }
    let hostname: String<40> = store
        .get_str(Key::Hostname)
        .await
        .unwrap_or_else(|| gateway_core::config::GatewayConfig::default().hostname);
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
    let configured: String<NTP_CAP> = config()
        .await
        .get_str(store::NTP_SERVER)
        .await
        .unwrap_or_default();
    if !configured.is_empty() {
        return configured;
    }
    crate::dhcpv6::ntp_server().unwrap_or_else(|| String::try_from(DEFAULT_NTP).unwrap_or_default())
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
    pub built_in_roots: Option<bool>,
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
        if server.len() > NTP_CAP || gateway_core::nts::parse_server(server).is_none() {
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
        if let Some(on) = update.built_in_roots {
            store
                .set_bool(store::BUILT_IN_ROOTS, on)
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
    // EST mode starts an enrollment, and so do other built-in roots: EST
    // may have been waiting for a CA to check its server against.
    if after.mode == Mode::Est || before.built_in_roots != after.built_in_roots {
        let mut est = EST.lock().await;
        est.failures = 0;
        est.next_attempt = None;
        drop(est);
        WAKE.signal(());
    }
    // The brokers checked against the CAs connect again with the new ones.
    if before.built_in_roots != after.built_in_roots {
        crate::mqtt::reconnect_all();
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

    let mut scratch = crate::https::scratch().await;
    let der = &mut scratch[..1024];
    let len = issue::self_signed(
        der,
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
    activate(&der[..len], origin).await?;
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
/// Each range refers to the decoded bytes in the upload workspace.
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

/// CA bundles and identity chains keep their input order. Both upload formats
/// leave the decoded DER at the start of the caller's buffer.
fn uploaded_chain(data: &mut [u8]) -> Result<&[u8], CertError> {
    let certs = decode_certificates(data)?;
    let mut end = 0;
    for range in certs {
        end = range.end;
        Certificate::parse(&data[range]).ok_or(CertError::Rejected(Problem::Unparsable))?;
    }
    Ok(&data[..end])
}

/// Validate and reorder a chain in the caller's separate output buffer.
async fn validated_chain<'a>(
    certs: impl Iterator<Item = &'a [u8]>,
    out: &mut [u8],
) -> Result<usize, CertError> {
    let point = key::point().await.ok_or(CertError::Key)?;
    let fqdn = effective_fqdn().await;
    let chain = check::order_chain(certs, &point).map_err(CertError::Rejected)?;
    check::check_leaf(&chain[0], &point, &fqdn, wall_clock::clock())
        .map_err(CertError::Rejected)?;
    let mut used = 0;
    for cert in &chain {
        let end = used + cert.der.len();
        out.get_mut(used..end)
            .ok_or(CertError::Rejected(Problem::TooLong))?
            .copy_from_slice(cert.der);
        used = end;
    }
    Ok(used)
}

/// Installs an uploaded certificate (chain), PEM or DER, after checking it,
/// and switches to the manual mode.
pub async fn install_certificate(data: &mut [u8], ordered: &mut [u8]) -> Result<(), CertError> {
    let certs = decode_certificates(data)?;
    let len = validated_chain(certs.iter().map(|range| &data[range.clone()]), ordered).await?;
    activate(&ordered[..len], Origin::Manual).await?;
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
pub(crate) async fn csr_pem() -> Result<(crate::https::Scratch, usize), CertError> {
    use embedded_io::Write as _;
    let fqdn = effective_fqdn().await;
    let mut der = [0; 512];
    let len = csr_der(&fqdn, &mut der).await?;
    let mut scratch = crate::https::scratch().await;
    let mut out = &mut scratch[..];
    write!(
        out,
        "{}",
        Pem {
            label: "CERTIFICATE REQUEST",
            der: &der[..len]
        }
    )
    .map_err(|_| CertError::NoMemory)?;
    let len = crate::https::SCRATCH_SIZE - out.len();
    Ok((scratch, len))
}

async fn csr_der(fqdn: &str, der: &mut [u8; 512]) -> Result<usize, CertError> {
    let point = key::point().await.ok_or(CertError::Key)?;
    let sign = key::signer().await.ok_or(CertError::Key)?;
    issue::csr(der, fqdn, &point, sign).map_err(|e| match e {
        issue::IssueError::BadFqdn => CertError::BadFqdn,
        _ => CertError::Key,
    })
}

/// Replaces the trust anchor (PEM or DER, one or more CA certificates);
/// empty data removes it.
pub async fn set_trust_anchor(data: &mut [u8]) -> Result<(), CertError> {
    if data.is_empty() {
        roots::replace(&[]).await.map_err(|_| CertError::Storage)?;
        crate::mqtt::reconnect_all();
        return Ok(());
    }
    let all = uploaded_chain(data)?;
    if all.len() > store::TRUST_ANCHOR.capacity() {
        return Err(CertError::Rejected(Problem::TooLong));
    }
    roots::replace(all).await.map_err(|_| CertError::Storage)?;
    // EST and the brokers may have been waiting for exactly this: try again
    // now.
    let mut est = EST.lock().await;
    est.failures = 0;
    est.next_attempt = None;
    drop(est);
    WAKE.signal(());
    crate::mqtt::reconnect_all();
    Ok(())
}

/// Installs the EST device identity: its certificate (chain) and private key,
/// both PEM in `data`. Empty data removes it. The key can't be read back.
pub async fn set_est_identity(data: &mut [u8], key_buf: &mut [u8]) -> Result<(), CertError> {
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
    let key_buf = Sensitive(key_buf);
    let key_len = {
        let text = core::str::from_utf8(data).map_err(|_| CertError::BadInput)?;
        let (_, body) = pem::blocks(text)
            .find(|(label, _)| label.ends_with("PRIVATE KEY"))
            .ok_or(CertError::BadInput)?;
        pem::decode(body.as_bytes(), key_buf.0).map_err(|_| CertError::BadInput)?
    };
    let key_der = &key_buf.0[..key_len];
    let chain = uploaded_chain(data)?;
    // MbedTLS must be able to use it.
    let usable = mbedtls_rs::PrivateKey::new(mbedtls_rs::X509::DER(key_der), None).is_ok();
    if !usable || key_der.len() > IDENTITY_KEY_CAP {
        Err(CertError::BadInput)
    } else if chain.len() > store::IDENTITY.capacity() {
        Err(CertError::Rejected(Problem::TooLong))
    } else {
        let stored = config()
            .await
            .set_bytes(store::IDENTITY_KEY, key_der)
            .await
            .map_err(|_| CertError::Storage);
        match stored {
            Ok(()) => store::IDENTITY
                .write(chain)
                .await
                .map_err(|_| CertError::Storage),
            Err(e) => Err(e),
        }
    }
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

/// The console and web UI take turns inspecting the three certificates.
#[derive(Default)]
pub struct CertificateDetails {
    pub certificate: Option<(CertInfo, check::Status, Origin)>,
    pub chain_length: usize,
    pub trust_anchor: Option<CertInfo>,
    pub trust_anchor_count: usize,
    pub est_identity: Option<CertInfo>,
}

// The TLS status page and explicit MQTT pin capture inspect certificates in
// this one RTC FAST slot. Other HTTPS requests and both MQTT sessions can run.
#[allow(clippy::large_enum_variant)]
enum Inspection {
    Details(CertificateDetails),
    Capture(heapless::Vec<u8, { crate::tls::CERT_CAP }>),
}

#[esp_hal::ram(unstable(rtc_fast))]
static INSPECTION: Mutex<CriticalSectionRawMutex, Inspection> =
    Mutex::new(Inspection::Capture(heapless::Vec::new()));

pub(crate) async fn capture_buffer(
) -> MappedMutexGuard<'static, CriticalSectionRawMutex, heapless::Vec<u8, { crate::tls::CERT_CAP }>>
{
    MutexGuard::map(INSPECTION.lock().await, |slot| {
        *slot = Inspection::Capture(heapless::Vec::new());
        match slot {
            Inspection::Capture(buf) => buf,
            Inspection::Details(_) => unreachable!(),
        }
    })
}

pub struct Status {
    pub settings: Settings,
    pub effective_fqdn: String<FQDN_CAP>,
    pub time_known: bool,
    pub details: MappedMutexGuard<'static, CriticalSectionRawMutex, CertificateDetails>,
    pub est: EstState,
    pub key_pin: Option<[u8; 32]>,
}

pub async fn status() -> Status {
    // All status readers take the inspection slot before scratch. The web reader keeps
    // that slot while it acquires scratch again to render the JSON response.
    let mut details = MutexGuard::map(INSPECTION.lock().await, |slot| {
        if let Inspection::Capture(_) = slot {
            *slot = Inspection::Details(CertificateDetails::default());
        }
        match slot {
            Inspection::Details(details) => details,
            Inspection::Capture(_) => unreachable!(),
        }
    });
    {
        let mut scratch = crate::https::scratch().await;
        let settings = settings().await;
        let fqdn = effective_fqdn().await;
        let clock = wall_clock::clock();
        let origin = Origin::from_u8(config().await.get_u8(store::ORIGIN).await.unwrap_or(0));

        let chain = store::CERTIFICATE.read_into(&mut scratch[..]).await;
        details.certificate = chain
            .as_deref()
            .and_then(check::stored_leaf)
            .map(|leaf| (info(&leaf), check::status(&leaf, &fqdn, clock), origin));
        details.chain_length = chain.map_or(0, |chain| split_concatenated(chain).count());
        let anchors = store::TRUST_ANCHOR.read_into(&mut scratch[..]).await;
        details.trust_anchor = anchors
            .as_deref()
            .and_then(check::stored_leaf)
            .map(|c| info(&c));
        details.trust_anchor_count =
            anchors.map_or(0, |anchors| split_concatenated(anchors).count());
        details.est_identity = store::IDENTITY
            .read_into(&mut scratch[..])
            .await
            .as_deref()
            .and_then(check::stored_leaf)
            .map(|c| info(&c));
        Status {
            settings,
            effective_fqdn: fqdn,
            time_known: clock.known().is_some(),
            details,
            est: EST.lock().await.clone(),
            key_pin: key_pin().await,
        }
    }
}

/// SHA-256 of the device key's `SubjectPublicKeyInfo`: its `pin-sha256`
/// (RFC 7469). The same for every certificate and CSR, as the key never
/// changes.
pub async fn key_pin() -> Option<[u8; 32]> {
    let point = key::point().await?;
    let mut spki = [0u8; 96];
    let len = issue::public_key_info(&mut spki, &point).ok()?;
    Some(crate::tls::fingerprint(&spki[..len]))
}

/// What the web server presents: the active chain and the device key, as
/// MbedTLS objects.
pub async fn server_credentials() -> Option<mbedtls_rs::Credentials<'static>> {
    let certificate = stored_certificate(&store::CERTIFICATE).await?;
    let private_key = key::mbedtls_key().await?;
    Some(mbedtls_rs::Credentials {
        certificate,
        private_key,
    })
}

async fn stored_certificate(blob: &store::Blob) -> Option<mbedtls_rs::Certificate<'static>> {
    let mut scratch = crate::https::scratch().await;
    let (der, pem) = scratch.split_at_mut(blob.capacity());
    let chain = blob.read_into(der).await?;
    mbedtls_certificate(split_concatenated(chain), pem)
}

/// MbedTLS accepts a single DER certificate or a PEM list. It copies the
/// certificates into its own pool before the workspace is released.
fn mbedtls_certificate<'a>(
    mut certs: impl Iterator<Item = &'a [u8]>,
    buffer: &mut [u8],
) -> Option<mbedtls_rs::Certificate<'static>> {
    use embedded_io::Write as _;
    let first = certs.next()?;
    let Some(second) = certs.next() else {
        return mbedtls_rs::Certificate::new(mbedtls_rs::X509::DER(first)).ok();
    };
    let mut out = &mut *buffer;
    for der in [first, second].into_iter().chain(certs) {
        write!(
            out,
            "{}",
            Pem {
                label: "CERTIFICATE",
                der
            }
        )
        .ok()?;
    }
    out.write_all(&[0]).ok()?;
    let remaining = out.len();
    let pem = core::ffi::CStr::from_bytes_with_nul(&buffer[..buffer.len() - remaining]).ok()?;
    mbedtls_rs::Certificate::new(mbedtls_rs::X509::PEM(pem)).ok()
}

/// Makes sure there is a key and a usable certificate; at boot, before the
/// web server starts.
pub async fn init() {
    roots::init().await;
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
    let mut scratch = crate::https::scratch().await;
    let usable = match store::CERTIFICATE.read_into(&mut scratch[..]).await {
        Some(chain) => match check::stored_leaf(chain) {
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
    drop(scratch);
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
        {
            ensure_usable().await;
            if settings().await.mode == Mode::Est {
                est_step(stack).await;
            }
        }
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
        let mut scratch = crate::https::scratch().await;
        let chain = store::CERTIFICATE.read_into(&mut scratch[..]).await;
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
    let mut slot = crate::https::slot().await;
    let trust = {
        let mut scratch = crate::https::scratch().await;
        let anchors = roots::Trusted::load().await;
        if anchors.is_empty() {
            return Err(EnrollError::Cert(CertError::NoTrustAnchor));
        }
        mbedtls_certificate(anchors.iter(), &mut scratch[..])
            .ok_or(EnrollError::Est(EstError::Tls))?
    };

    let mut csr = [0; 512];
    let csr_len = csr_der(fqdn, &mut csr).await.map_err(EnrollError::Cert)?;

    // Renewal authenticates with the certificate being renewed (RFC 7030
    // 4.2.2); a first enrollment with the device identity, if there is one.
    let client = if renewing {
        server_credentials().await
    } else {
        identity_credentials().await
    };

    let mut scratch = crate::https::scratch().await;
    let p7_len = est::exchange(
        stack,
        url,
        operation,
        &csr[..csr_len],
        est::Auth { trust, client },
        &mut slot,
        &mut scratch[..],
    )
    .await
    .map_err(EnrollError::Est)?;
    let (reply, ordered) =
        scratch.split_at_mut(crate::https::SCRATCH_SIZE - store::CERTIFICATE.capacity());
    let p7 = reply
        .get(..p7_len)
        .ok_or(EnrollError::Est(EstError::Malformed))?;
    let certs = pkcs7::certificates(p7).ok_or(EnrollError::Est(EstError::Malformed))?;
    let len = validated_chain(certs, ordered)
        .await
        .map_err(EnrollError::Cert)?;
    activate(&ordered[..len], Origin::Est)
        .await
        .map_err(EnrollError::Cert)?;
    console_log!(
        "https: EST {} for {fqdn}: certificate installed",
        operation.path()
    );
    Ok(())
}

async fn identity_credentials() -> Option<mbedtls_rs::Credentials<'static>> {
    let certificate = stored_certificate(&store::IDENTITY).await?;
    let mut scratch = crate::https::scratch().await;
    let der = Sensitive(&mut scratch[..IDENTITY_KEY_CAP]);
    let len = config()
        .await
        .read_bytes(store::IDENTITY_KEY, der.0)
        .await
        .filter(|n| *n > 0);
    let key = len.and_then(|len| {
        mbedtls_rs::PrivateKey::new(mbedtls_rs::X509::DER(&der.0[..len]), None).ok()
    });
    Some(mbedtls_rs::Credentials {
        certificate,
        private_key: key?,
    })
}
