//! `/api/v1/config`: the gateway's own settings.
//!
//! The web password is write-only: it can be set, never read back.

use super::error::ApiError;
use super::json::json;
use crate::config::{self, config, LoadOrInit};
use gateway_core::web::{strict_bitrate, valid_hostname, MIN_PASSWORD_LEN};
use heapless::String;
use picoserve::extract::Json;
use picoserve::response::IntoResponse;
use picoserve::routing::{get, PathRouter};
use picoserve::Router;
use serde::{Deserialize, Serialize};

pub fn routes<R: PathRouter>(router: Router<R>) -> Router<impl PathRouter> {
    router.route("/config", get(read).patch(update))
}

#[derive(Serialize)]
struct Config {
    hostname: String<40>,
    can_bitrate: &'static str,
    update_delay_ms: u32,
    /// GPIO of the Ethernet PHY's RMII clock: 16 or 17.
    eth_clock: u8,
    web_username: String<30>,
    web_password_set: bool,
}

async fn read() -> Result<impl IntoResponse, ApiError> {
    let cfg = config().await.load_or_init().await;
    json(&Config {
        hostname: cfg.hostname,
        can_bitrate: cfg.can_bitrate.as_str(),
        update_delay_ms: cfg.update_delay_ms,
        eth_clock: config::eth_clock_gpio().await,
        web_username: cfg.web_username,
        web_password_set: !cfg.web_password.is_empty(),
    })
    .await
}

/// Every field is optional; only the given ones change.
#[derive(Deserialize)]
struct Update {
    hostname: Option<String<40>>,
    can_bitrate: Option<String<8>>,
    update_delay_ms: Option<u32>,
    eth_clock: Option<u8>,
    web_username: Option<String<30>>,
    web_password: Option<String<30>>,
}

#[derive(Serialize)]
struct Updated {
    /// Hostname, bitrate and RMII clock are only read at boot.
    restart_required: bool,
}

async fn update(Json(body): Json<Update>) -> Result<impl IntoResponse, ApiError> {
    // Check everything before changing anything.
    if let Some(hostname) = &body.hostname {
        if !valid_hostname(hostname) {
            return Err(ApiError::invalid(
                "hostname: 1-40 letters, digits and hyphens",
            ));
        }
    }
    let bitrate = match &body.can_bitrate {
        Some(name) => Some(
            strict_bitrate(name)
                .ok_or(ApiError::invalid("can_bitrate: b22_222, b25, b50 or b100"))?,
        ),
        None => None,
    };
    if let Some(ms) = body.update_delay_ms {
        if !(1..=10_000).contains(&ms) {
            return Err(ApiError::invalid("update_delay_ms: 1-10000"));
        }
    }
    if let Some(gpio) = body.eth_clock {
        if gpio != 16 && gpio != 17 {
            return Err(ApiError::invalid("eth_clock: 16 or 17"));
        }
    }
    if body.web_username.as_ref().is_some_and(|u| u.is_empty()) {
        return Err(ApiError::invalid("web_username must not be empty"));
    }
    if let Some(password) = &body.web_password {
        if password.len() < MIN_PASSWORD_LEN {
            return Err(ApiError::invalid("web_password: at least 8 characters"));
        }
    }

    let failed = ApiError::new(
        picoserve::response::StatusCode::INTERNAL_SERVER_ERROR,
        "storage",
        "could not store the setting",
    );
    let mut store = config().await;
    let mut restart_required = false;
    if let Some(hostname) = &body.hostname {
        store
            .set_str(config::Key::Hostname, hostname)
            .await
            .map_err(|_| failed)?;
        restart_required = true;
    }
    if let Some(bitrate) = bitrate {
        store
            .set_u8(config::Key::CanBitrate, bitrate.into())
            .await
            .map_err(|_| failed)?;
        restart_required = true;
    }
    if let Some(ms) = body.update_delay_ms {
        store
            .set_u32(config::Key::UpdateDelay, ms)
            .await
            .map_err(|_| failed)?;
    }
    drop(store);
    if let Some(gpio) = body.eth_clock {
        config::set_eth_clock_gpio(gpio).await.map_err(|_| failed)?;
        restart_required = true;
    }
    let mut store = config().await;
    if let Some(user) = &body.web_username {
        store
            .set_str(config::Key::WebUsername, user)
            .await
            .map_err(|_| failed)?;
    }
    if let Some(password) = &body.web_password {
        store
            .set_str(config::Key::WebPassword, password)
            .await
            .map_err(|_| failed)?;
    }
    json(&Updated { restart_required }).await
}
