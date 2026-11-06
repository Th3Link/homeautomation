//! `/api/v1/network`: the network settings ([`gateway_core::network`]).
//! What the network is doing right now is in `/system`.

use super::error::ApiError;
use super::json::json;
use crate::config;
use gateway_core::network::{link_local_v4, NetworkConfig};
use heapless::String;
use picoserve::extract::Json;
use picoserve::response::{IntoResponse, StatusCode};
use picoserve::routing::{get, PathRouter};
use picoserve::Router;
use serde::Serialize;

pub fn routes<R: PathRouter>(router: Router<R>) -> Router<impl PathRouter> {
    router.route("/network", get(read).put(update))
}

#[derive(Serialize)]
struct Network {
    settings: NetworkConfig,
    /// The address link-local IPv4 takes, from the MAC.
    link_local_ipv4: String<15>,
    /// The stored settings differ from those the network was started
    /// with: they take effect after a restart.
    restart_required: bool,
}

async fn read() -> Result<impl IntoResponse, ApiError> {
    crate::heap::boxed(|| async move {
        let settings = config::network().await;
        let running = crate::ethernet::info().await.map(|net| net.settings);
        let mac: [u8; 6] = esp_hal::efuse::base_mac_address()
            .as_bytes()
            .try_into()
            .unwrap_or([0; 6]);
        let mut link_local_ipv4 = String::new();
        let _ = core::fmt::write(&mut link_local_ipv4, format_args!("{}", link_local_v4(mac)));
        json(&Network {
            restart_required: running.is_some_and(|running| running != settings),
            settings,
            link_local_ipv4,
        })
        .await
    })
    .await
}

/// Takes all the settings at once.
async fn update(Json(settings): Json<NetworkConfig>) -> Result<impl IntoResponse, ApiError> {
    crate::heap::boxed(|| async move {
        settings.validate().map_err(ApiError::invalid)?;
        config::set_network(&settings).await.map_err(|()| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage",
                "could not store the setting",
            )
        })?;
        crate::console_log!("network: settings changed (take effect after a restart)");
        Ok(())
    })
    .await?;
    read().await
}
