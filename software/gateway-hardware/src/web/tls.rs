//! `/api/v1/tls`: the HTTPS certificate (ADR 0018). Everything is done by
//! [`crate::cert`]; this is only its REST face.
//!
//! - `GET /tls`: settings, the active certificate, trust anchor, EST state.
//! - `PATCH /tls`: settings (FQDN, mode, validity, EST URL/renewal, NTP).
//! - `POST /tls/self-signed`: make a new self-signed certificate.
//! - `GET /tls/csr`: the CSR for the device key and the FQDN (PEM).
//! - `PUT /tls/certificate`: install a signed certificate (chain), PEM/DER.
//! - `PUT|DELETE /tls/trust-anchor`: the CA(s) the EST server must chain to.
//! - `PUT|DELETE /tls/est/identity`: the EST device identity, one PEM body
//!   with the certificate(s) and the private key (which can't be read back).
//! - `POST /tls/est/enroll`: enroll/renew now.
//!
//! There is no route that returns a private key.

use super::error::ApiError;
use crate::cert::{self, CertError, CertInfo, Mode, SettingsUpdate};
use alloc::vec::Vec;
use core::fmt::Write as _;
use gateway_core::x509::pem::Base64;
use gateway_core::x509::time::Rfc3339;
use heapless::String;
use picoserve::extract::Json;
use picoserve::io::Read;
use picoserve::request::Request;
use picoserve::response::{IntoResponse, ResponseWriter, StatusCode};
use picoserve::routing::{get, post, PathRouter, RequestHandlerService};
use picoserve::{ResponseSent, Router};
use serde::{Deserialize, Serialize};

/// The largest certificate/key upload: room for what can be stored (6 KiB of
/// DER, as PEM), and small enough for the heap next to the TLS session.
const UPLOAD_MAX: usize = 10 * 1024;

pub fn routes<R: PathRouter>(router: Router<R>) -> Router<impl PathRouter> {
    router
        .route("/tls", get(read).patch(update))
        .route("/tls/self-signed", post(self_signed))
        .route("/tls/csr", get(csr))
        .route(
            "/tls/certificate",
            picoserve::routing::put_service(Upload(Target::Certificate)),
        )
        .route(
            "/tls/trust-anchor",
            picoserve::routing::put_service(Upload(Target::TrustAnchor))
                .delete(delete_trust_anchor),
        )
        .route(
            "/tls/est/identity",
            picoserve::routing::put_service(Upload(Target::Identity)).delete(delete_identity),
        )
        .route("/tls/est/enroll", post(enroll))
}

fn api_error(error: CertError) -> ApiError {
    let status = match error {
        CertError::Storage | CertError::Key => StatusCode::INTERNAL_SERVER_ERROR,
        CertError::NoMemory => StatusCode::SERVICE_UNAVAILABLE,
        CertError::Rejected(_) => StatusCode::UNPROCESSABLE_ENTITY,
        _ => StatusCode::BAD_REQUEST,
    };
    let message = match error {
        CertError::Rejected(gateway_core::x509::check::Problem::KeyMismatch) => {
            "the certificate is not for this gateway's key"
        }
        CertError::Rejected(gateway_core::x509::check::Problem::FqdnMismatch) => {
            "the certificate does not name the configured FQDN"
        }
        CertError::Rejected(gateway_core::x509::check::Problem::Expired) => {
            "the certificate has expired"
        }
        CertError::Rejected(gateway_core::x509::check::Problem::NotYetValid) => {
            "the certificate is not valid yet"
        }
        CertError::Rejected(gateway_core::x509::check::Problem::TooLong) => {
            "the certificate chain is too long"
        }
        CertError::Rejected(gateway_core::x509::check::Problem::Unparsable) => "not a certificate",
        CertError::BadFqdn => "fqdn: letters, digits, hyphens and dots, at most 253",
        CertError::BadUrl => "est_url: https://host[:port][/path]",
        CertError::BadValidity => "validity_years: 1-30",
        CertError::BadInput => "expected PEM or DER",
        CertError::NoTrustAnchor => "no trusted CA (built-in roots off, none installed)",
        CertError::Key => "the device key is not usable",
        CertError::Storage => "could not store",
        CertError::NoMemory => "out of memory",
    };
    ApiError::new(status, error.code(), message)
}

type Text<const N: usize> = String<N>;

fn time_text(seconds: i64) -> Text<20> {
    let mut text = String::new();
    let _ = write!(text, "{}", Rfc3339(seconds));
    text
}

fn hex(bytes: &[u8; 32]) -> Text<64> {
    let mut text = String::new();
    for byte in bytes {
        let _ = write!(text, "{byte:02x}");
    }
    text
}

fn base64(bytes: &[u8; 32]) -> Text<44> {
    let mut text = String::new();
    let _ = write!(text, "{}", Base64 { bytes, line: 0 });
    text
}

#[derive(Serialize)]
struct CertView<'a> {
    subject: &'a str,
    issuer: &'a str,
    not_before: Text<20>,
    not_after: Text<20>,
    dns_names: &'a [String<{ cert::FQDN_CAP }>],
    fingerprint_sha256: Text<64>,
    /// Active certificate: `valid`, `expires_soon`, `expired`, …
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<&'static str>,
    /// Active certificate: how it was made.
    #[serde(skip_serializing_if = "Option::is_none")]
    origin: Option<&'static str>,
    /// Active certificate: certificates in the chain; trust anchor: how
    /// many CA certificates are installed.
    #[serde(skip_serializing_if = "Option::is_none")]
    count: Option<usize>,
}

impl<'a> CertView<'a> {
    fn new(info: &'a CertInfo) -> Self {
        Self {
            subject: &info.subject,
            issuer: &info.issuer,
            not_before: time_text(info.not_before),
            not_after: time_text(info.not_after),
            dns_names: &info.dns_names,
            fingerprint_sha256: hex(&info.fingerprint),
            status: None,
            origin: None,
            count: None,
        }
    }
}

#[derive(Serialize)]
struct EstView<'a> {
    url: &'a str,
    auto_renew: bool,
    identity: Option<CertView<'a>>,
    last_result: Option<&'static str>,
    last_status: Option<u16>,
    last_attempt: Option<Text<20>>,
    failures: u32,
    next_attempt_in_s: Option<u64>,
}

#[derive(Serialize)]
struct StatusView<'a> {
    fqdn: &'a str,
    effective_fqdn: &'a str,
    mode: &'static str,
    validity_years: u8,
    https: bool,
    time_known: bool,
    ntp_server: &'a str,
    ntp_server_in_use: &'a str,
    /// The device key's `pin-sha256` (RFC 7469), base64.
    key_pin_sha256: Option<Text<44>>,
    certificate: Option<CertView<'a>>,
    trust_anchor: Option<CertView<'a>>,
    /// Whether the built-in root certificates are trusted, and which.
    built_in_roots: bool,
    built_in_root_names: [&'static str; cert::roots::BUILT_IN.len()],
    est: EstView<'a>,
}

async fn read() -> Result<impl IntoResponse, ApiError> {
    let status = cert::status().await;
    let ntp_server_in_use = cert::ntp_server().await;
    let now = embassy_time::Instant::now();
    let view = StatusView {
        fqdn: &status.settings.fqdn,
        effective_fqdn: &status.effective_fqdn,
        mode: status.settings.mode.code(),
        validity_years: status.settings.validity_years,
        https: status.https_enabled,
        time_known: status.time_known,
        ntp_server: &status.settings.ntp_server,
        ntp_server_in_use: &ntp_server_in_use,
        key_pin_sha256: status.key_pin.as_ref().map(base64),
        certificate: status
            .certificate
            .as_ref()
            .map(|(info, state, origin)| CertView {
                status: Some(state.code()),
                origin: Some(origin.code()),
                count: Some(status.chain_length),
                ..CertView::new(info)
            }),
        trust_anchor: status.trust_anchor.as_ref().map(|info| CertView {
            count: Some(status.trust_anchor_count),
            ..CertView::new(info)
        }),
        built_in_roots: status.settings.built_in_roots,
        built_in_root_names: cert::roots::BUILT_IN.map(|root| root.name),
        est: EstView {
            url: &status.settings.est_url,
            auto_renew: status.settings.est_auto_renew,
            identity: status.est_identity.as_ref().map(CertView::new),
            last_result: status.est.last_result,
            last_status: status.est.last_status,
            last_attempt: (status.est.last_attempt != 0)
                .then(|| time_text(status.est.last_attempt)),
            failures: status.est.failures,
            next_attempt_in_s: status
                .est
                .next_attempt
                .map(|at| at.saturating_duration_since(now).as_secs()),
        },
    };
    let body = super::json::acquire().await.rendered(&view)?;
    Ok(picoserve::response::Response::new(StatusCode::OK, body))
}

#[derive(Deserialize)]
struct Update {
    fqdn: Option<String<{ cert::FQDN_CAP }>>,
    mode: Option<String<16>>,
    validity_years: Option<u8>,
    est_url: Option<String<{ cert::URL_CAP }>>,
    est_auto_renew: Option<bool>,
    ntp_server: Option<String<{ cert::NTP_CAP }>>,
    built_in_roots: Option<bool>,
}

async fn update(Json(body): Json<Update>) -> Result<impl IntoResponse, ApiError> {
    let mode = match &body.mode {
        Some(code) => Some(
            Mode::from_code(code).ok_or(ApiError::invalid("mode: self_signed, manual or est"))?,
        ),
        None => None,
    };
    crate::heap::boxed(|| {
        cert::update_settings(SettingsUpdate {
            fqdn: body.fqdn.as_deref(),
            mode,
            validity_years: body.validity_years,
            est_url: body.est_url.as_deref(),
            est_auto_renew: body.est_auto_renew,
            ntp_server: body.ntp_server.as_deref(),
            built_in_roots: body.built_in_roots,
        })
    })
    .await
    .map_err(api_error)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, Default)]
struct SelfSignedRequest {
    validity_years: Option<u8>,
}

async fn self_signed(Json(body): Json<SelfSignedRequest>) -> Result<impl IntoResponse, ApiError> {
    crate::heap::boxed(|| async move {
        cert::generate_self_signed(body.validity_years)
            .await
            .map_err(api_error)?;
        Ok(StatusCode::NO_CONTENT)
    })
    .await
}

/// The CSR, as `application/pkcs10` PEM, offered as a file.
struct CsrBody(alloc::string::String);

impl picoserve::response::Content for CsrBody {
    fn content_type(&self) -> &'static str {
        "application/pkcs10"
    }

    fn content_length(&self) -> usize {
        self.0.len()
    }

    async fn write_content<W: picoserve::io::Write>(self, mut writer: W) -> Result<(), W::Error> {
        writer.write_all(self.0.as_bytes()).await
    }
}

async fn csr() -> Result<impl IntoResponse, ApiError> {
    crate::heap::boxed(|| async move {
        let pem = cert::csr_pem().await.map_err(api_error)?;
        Ok((
            [(
                "Content-Disposition",
                "attachment; filename=\"gateway.csr\"",
            )],
            CsrBody(pem),
        ))
    })
    .await
}

async fn delete_trust_anchor() -> Result<impl IntoResponse, ApiError> {
    cert::set_trust_anchor(&mut []).await.map_err(api_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_identity() -> Result<impl IntoResponse, ApiError> {
    cert::set_est_identity(&mut []).await.map_err(api_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn enroll() -> Result<impl IntoResponse, ApiError> {
    if cert::settings().await.mode != Mode::Est {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "not_est_mode",
            "the certificate mode is not est",
        ));
    }
    cert::request_enrollment().await;
    Ok(StatusCode::ACCEPTED)
}

#[derive(Clone, Copy)]
enum Target {
    Certificate,
    TrustAnchor,
    Identity,
}

/// A certificate (or identity) upload: the raw request body.
struct Upload(Target);

impl RequestHandlerService<(), ()> for Upload {
    async fn call_request_handler_service<R: Read, W: ResponseWriter<Error = R::Error>>(
        &self,
        _state: &(),
        _path_parameters: (),
        mut request: Request<'_, R>,
        response_writer: W,
    ) -> Result<ResponseSent, W::Error> {
        let size = request.body_connection.content_length();
        let body = if size == 0 || size > UPLOAD_MAX {
            Err(ApiError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "bad_size",
                "the body must be 1 byte to 10 KiB",
            ))
        } else {
            let mut data: Vec<u8> = Vec::new();
            if data.try_reserve_exact(size).is_err() {
                Err(api_error(CertError::NoMemory))
            } else {
                data.resize(size, 0);
                let mut reader = request.body_connection.body().reader();
                match reader.read_exact(&mut data).await {
                    Ok(()) => Ok(data),
                    Err(_) => Err(ApiError::invalid("the body ended early")),
                }
            }
        };
        let connection = request.body_connection.finalize().await?;

        let result = match body {
            Err(error) => Err(error),
            Ok(mut data) => {
                let result = match self.0 {
                    Target::Certificate => cert::install_certificate(&mut data).await,
                    Target::TrustAnchor => cert::set_trust_anchor(&mut data).await,
                    Target::Identity => cert::set_est_identity(&mut data).await,
                };
                // An identity upload holds a private key.
                crate::cert::wipe(&mut data);
                result.map_err(api_error)
            }
        };
        match result {
            Ok(()) => {
                StatusCode::NO_CONTENT
                    .write_to(connection, response_writer)
                    .await
            }
            Err(error) => error.write_to(connection, response_writer).await,
        }
    }
}
