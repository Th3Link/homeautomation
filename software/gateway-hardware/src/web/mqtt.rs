//! `/api/v1/mqtt`: the two brokers (`primary` is reported to, `secondary`
//! only listens, see `gateway-core/docs/protocol-mqtt.md`) and the TLS
//! trust of each.
//!
//! Passwords are write-only. Changing a broker's settings reconnects it.

use super::error::ApiError;
use super::json::json;
use crate::config::{self, config, LoadOrInit};
use core::str::FromStr;
use gateway_core::config::{BrokerConfig, MQTT_BROKERS};
use gateway_core::helper::parse_broker_uri;
use gateway_core::web::MIN_PASSWORD_LEN;
use heapless::String;
use picoserve::extract::Json;
use picoserve::response::{IntoResponse, StatusCode};
use picoserve::routing::{get, parse_path_segment, post, PathRouter};
use picoserve::Router;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub fn routes<R: PathRouter>(router: Router<R>) -> Router<impl PathRouter> {
    let slot = || parse_path_segment::<Slot>();
    router
        .route("/mqtt", get(read_all).patch(update_all))
        .route(("/mqtt", slot()), get(read).patch(update))
        .route(("/mqtt", slot(), "/trust"), post(trust))
        .route(
            ("/mqtt", slot(), "/certificate"),
            get(certificate).delete(forget),
        )
}

/// The `:slot` path segment: `primary` or `secondary`.
pub struct Slot(usize);

impl FromStr for Slot {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "primary" => Ok(Self(crate::mqtt::PRIMARY)),
            "secondary" => Ok(Self(crate::mqtt::SECONDARY)),
            _ => Err(()),
        }
    }
}

const fn role(slot: usize) -> &'static str {
    if slot == crate::mqtt::PRIMARY {
        "primary"
    } else {
        "secondary"
    }
}

#[derive(Serialize)]
struct Broker {
    slot: &'static str,
    enabled: bool,
    uri: String<60>,
    user: String<60>,
    password_set: bool,
    /// Accept any certificate on `mqtts://` instead of the pinned one.
    tls_insecure: bool,
    certificate_pinned: bool,
    connected: bool,
}

async fn view(slot: usize, settings: BrokerConfig) -> Broker {
    Broker {
        slot: role(slot),
        enabled: settings.enabled,
        uri: settings.uri,
        user: settings.user,
        password_set: !settings.password.is_empty(),
        tls_insecure: settings.tls_insecure,
        certificate_pinned: config::pinned_certificate(slot).await.is_some(),
        connected: crate::mqtt::is_connected(slot),
    }
}

#[derive(Serialize)]
struct Brokers {
    primary: Broker,
    secondary: Broker,
    /// Home Assistant discovery on the primary broker ([`crate::ha`]).
    home_assistant: bool,
}

async fn read_all() -> Result<impl IntoResponse, ApiError> {
    crate::heap::boxed(|| async move {
        let cfg = config().await.load_or_init().await;
        let [primary, secondary] = cfg.brokers;
        json(&Brokers {
            primary: view(crate::mqtt::PRIMARY, primary).await,
            secondary: view(crate::mqtt::SECONDARY, secondary).await,
            home_assistant: crate::ha::enabled(),
        })
        .await
    })
    .await
}

async fn read(Slot(slot): Slot) -> Result<impl IntoResponse, ApiError> {
    crate::heap::boxed(|| async move {
        let cfg = config().await.load_or_init().await;
        json(&view(slot, cfg.brokers[slot].clone()).await).await
    })
    .await
}

/// The settings of both brokers together.
#[derive(Deserialize)]
struct UpdateAll {
    home_assistant: Option<bool>,
}

async fn update_all(Json(body): Json<UpdateAll>) -> Result<impl IntoResponse, ApiError> {
    if let Some(on) = body.home_assistant {
        crate::ha::set_enabled(on).await.map_err(|()| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage",
                "the setting could not be stored",
            )
        })?;
    }
    read_all().await
}

/// Every field is optional; only the given ones change.
#[derive(Deserialize)]
struct Update {
    enabled: Option<bool>,
    /// `mqtt://host[:port]` or `mqtts://host[:port]`.
    uri: Option<String<60>>,
    user: Option<String<60>>,
    /// At least 8 characters, or empty for none.
    password: Option<String<60>>,
    tls_insecure: Option<bool>,
}

async fn update(Slot(slot): Slot, Json(body): Json<Update>) -> Result<impl IntoResponse, ApiError> {
    crate::heap::boxed(|| async move {
        if let Some(uri) = &body.uri {
            if parse_broker_uri(uri).is_none() {
                return Err(ApiError::invalid(
                    "uri: mqtt://host[:port] or mqtts://host[:port]",
                ));
            }
        }
        if let Some(password) = &body.password {
            if !password.is_empty() && password.len() < MIN_PASSWORD_LEN {
                return Err(ApiError::invalid(
                    "password: at least 8 characters, or empty for none",
                ));
            }
        }

        let keys = gateway_core::config::Key::broker(slot);
        let failed = ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "storage",
            "could not store the setting",
        );
        {
            let mut store = config().await;
            if let Some(uri) = &body.uri {
                store.set_str(keys.uri, uri).await.map_err(|_| failed)?;
            }
            if let Some(user) = &body.user {
                store.set_str(keys.user, user).await.map_err(|_| failed)?;
            }
            if let Some(password) = &body.password {
                store
                    .set_str(keys.password, password)
                    .await
                    .map_err(|_| failed)?;
            }
            if let Some(enabled) = body.enabled {
                store
                    .set_bool(keys.enabled, enabled)
                    .await
                    .map_err(|_| failed)?;
            }
            if let Some(insecure) = body.tls_insecure {
                store
                    .set_bool(keys.tls_insecure, insecure)
                    .await
                    .map_err(|_| failed)?;
            }
        }
        crate::mqtt::reconnect(slot);
        read(Slot(slot)).await
    })
    .await
}

/// The next connection to the broker takes the certificate it presents and
/// pins it; see `gateway-hardware/src/tls.rs`.
async fn trust(Slot(slot): Slot) -> impl IntoResponse {
    crate::mqtt::request_trust(slot);
    crate::mqtt::reconnect(slot);
    StatusCode::ACCEPTED
}

#[derive(Serialize)]
struct Certificate {
    pinned: bool,
    /// SHA-256 of the pinned certificate (DER), hexadecimal: what
    /// `openssl x509 -noout -fingerprint -sha256` shows.
    sha256: Option<String<64>>,
}

async fn certificate(Slot(slot): Slot) -> Result<impl IntoResponse, ApiError> {
    let sha256 = config::pinned_certificate(slot).await.map(|cert| {
        let mut hex = String::new();
        for byte in Sha256::digest(&cert) {
            let _ = core::fmt::write(&mut hex, format_args!("{byte:02x}"));
        }
        hex
    });
    json(&Certificate {
        pinned: sha256.is_some(),
        sha256,
    })
    .await
}

async fn forget(Slot(slot): Slot) -> Result<impl IntoResponse, ApiError> {
    config::set_pinned_certificate(slot, &[])
        .await
        .map_err(|_| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage",
                "could not forget the certificate",
            )
        })?;
    crate::mqtt::reconnect(slot);
    Ok(StatusCode::NO_CONTENT)
}

// `MQTT_BROKERS` must stay 2: the paths name the two slots.
const _: () = assert!(MQTT_BROKERS == 2);
