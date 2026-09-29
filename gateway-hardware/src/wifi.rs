//! WiFi bring-up: station mode with a fallback to a setup access point,
//! mirroring `WiFi.cpp`'s intent — but not its literal disconnect-handling
//! bug. In the original, `WIFI_EVENT_STA_DISCONNECTED`'s handler
//! unconditionally schedules `esp_restart()` 4 seconds after **any**
//! disconnect (the delay/restart calls sit *after*, not inside, the
//! `if (retry < MAX)`/`else` branch), so in practice the device reboots on
//! the very first disconnect regardless of retry count — the "fall back to
//! a setup AP without restarting" path the retry-counting logic seems to
//! build toward is effectively dead code, raced by that unconditional
//! restart. Reproducing that faithfully would just make the gateway
//! reboot-loop on any WiFi hiccup, so this deliberately implements the
//! evident *intent* instead: retry a bounded number of times, then either
//! give up and bring up the setup AP (first connection) or leave the
//! existing connection alone and let embassy-net's own retry/DHCP renewal
//! handle transient drops (steady state).

use embassy_executor::Spawner;
use embassy_net::{Runner, Stack, StackResources};
use esp_radio::wifi::ap::AccessPointConfig;
use esp_radio::wifi::sta::StationConfig;
use esp_radio::wifi::{
    AuthenticationMethodConfig, Config as WifiConfig, Interface, Password, Ssid, WifiController,
};
use gateway_core::config::WifiMode;
use heapless::String;
use static_cell::StaticCell;

use crate::console_log;

/// Matches `WiFi.cpp`'s `EXAMPLE_ESP_MAXIMUM_RETRY`.
const MAX_STA_RETRIES: u8 = 4;
/// Fixed fallback credentials, matching `WiFi::read_nvs`'s hardcoded
/// setup-AP values (also [`gateway_core::config::GatewayConfig`]'s
/// defaults).
const FALLBACK_SSID: &str = "CAN2MQTTSETUP";
const FALLBACK_PASSWORD: &str = "Can2MqttPass";
/// `WiFi::init`'s `init_softap(10)` channel.
const AP_CHANNEL: u8 = 10;
/// `WiFi::init_client`'s SSID-plus-suffix on AP fallback.
const AP_SUFFIX: &str = "_ap";

static RESOURCES: StaticCell<StackResources<4>> = StaticCell::new();

/// What WiFi bring-up actually settled on — the caller ([`init_stack`])
/// needs this to pick a static (AP) vs. DHCP (station) network config.
pub enum Outcome {
    Station(Interface),
    AccessPoint(Interface),
}

/// Brings up WiFi per `mode`/`ssid`/`password` (already resolved through
/// [`gateway_core::config::resolve_wifi_credentials`] by the caller): tries
/// station mode with up to `MAX_STA_RETRIES` connection attempts, falling
/// back to a `<ssid>_ap` access point (matching `WiFi::init_client`'s
/// `strcat(m_ssid, "_ap")`) if every attempt fails. `mode ==
/// `[`WifiMode::AccessPoint`] skips straight to the access point;
/// `[`WifiMode::Off`] returns `None` (WiFi stays down).
pub async fn bring_up(
    wifi: esp_hal::peripherals::WIFI<'static>,
    mode: WifiMode,
    ssid: &str,
    password: &str,
) -> Option<(Outcome, WifiController<'static>)> {
    let mut controller = match WifiController::new(wifi, Default::default()) {
        Ok(c) => c,
        Err(e) => {
            console_log!("wifi: controller init failed: {e:?}");
            return None;
        }
    };

    match mode {
        WifiMode::Off => None,
        WifiMode::Client => {
            let interface = Interface::station();
            if try_station(&mut controller, ssid, password).await {
                Some((Outcome::Station(interface), controller))
            } else {
                console_log!("wifi: station connect failed after {MAX_STA_RETRIES} attempts, falling back to setup AP");
                drop(interface);
                let ap_interface = Interface::access_point();
                let mut ap_ssid: String<40> = String::new();
                let _ = ap_ssid.push_str(ssid);
                let _ = ap_ssid.push_str(AP_SUFFIX);
                start_access_point(&mut controller, &ap_ssid, password).await;
                Some((Outcome::AccessPoint(ap_interface), controller))
            }
        }
        WifiMode::AccessPoint => {
            let interface = Interface::access_point();
            start_access_point(&mut controller, ssid, password).await;
            Some((Outcome::AccessPoint(interface), controller))
        }
    }
}

async fn try_station(controller: &mut WifiController<'static>, ssid: &str, password: &str) -> bool {
    let Ok(ssid) = Ssid::try_from(ssid) else {
        console_log!("wifi: invalid SSID");
        return false;
    };
    let auth = match Password::try_from(password) {
        Ok(pw) => AuthenticationMethodConfig::Wpa2Personal(pw),
        Err(_) => AuthenticationMethodConfig::Open,
    };
    let station_config = StationConfig::default()
        .with_ssid(ssid)
        .with_authentication(auth);

    if let Err(e) = controller.set_config(&WifiConfig::Station(station_config)) {
        console_log!("wifi: set_config failed: {e:?}");
        return false;
    }

    for attempt in 1..=MAX_STA_RETRIES {
        console_log!("wifi: connecting (attempt {attempt}/{MAX_STA_RETRIES})");
        match controller.connect_async().await {
            Ok(_) => return true,
            Err(e) => console_log!("wifi: connect failed: {e:?}"),
        }
    }
    false
}

/// Brings up the access point. Falls back to [`FALLBACK_SSID`]/
/// [`FALLBACK_PASSWORD`] if `ssid`/`password` don't fit WiFi's limits
/// (32-byte SSID, 8-63 byte WPA2 password) — matching the spirit of
/// `WiFi::read_nvs`'s "invalid config forces the hardcoded setup AP"
/// fallback. An empty `password` brings up an open (unencrypted) AP,
/// matching `WiFi::init_softap`'s `WIFI_AUTH_OPEN` branch.
async fn start_access_point(controller: &mut WifiController<'static>, ssid: &str, password: &str) {
    let ssid = Ssid::try_from(ssid)
        .or_else(|_| Ssid::try_from(FALLBACK_SSID))
        .expect("fallback SSID is always valid");
    let auth = if password.is_empty() {
        AuthenticationMethodConfig::Open
    } else {
        match Password::try_from(password) {
            Ok(pw) => AuthenticationMethodConfig::Wpa2Personal(pw),
            Err(_) => AuthenticationMethodConfig::Wpa2Personal(
                Password::try_from(FALLBACK_PASSWORD).expect("fallback password is always valid"),
            ),
        }
    };

    let ap_config = AccessPointConfig::default()
        .with_ssid(ssid)
        .with_authentication(auth)
        .with_channel(AP_CHANNEL)
        .with_max_connections(4);

    if let Err(e) = controller.set_config(&WifiConfig::AccessPoint(ap_config)) {
        console_log!("wifi: access point set_config failed: {e:?}");
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, Interface>) {
    runner.run().await
}

/// Starts the `embassy-net` stack over `interface` — DHCP client for a
/// station interface, a fixed `192.168.4.1/24` for the access point (this
/// gateway doesn't implement a DHCP *server*, so a client associating with
/// the fallback AP needs a static IP in that range to reach the web UI;
/// `embassy-net` 0.9 has no server-side DHCP support to draw on here).
pub async fn init_stack(outcome: Outcome, spawner: &Spawner) -> Stack<'static> {
    let (interface, net_config) = match outcome {
        Outcome::Station(interface) => (interface, embassy_net::Config::dhcpv4(Default::default())),
        Outcome::AccessPoint(interface) => {
            use embassy_net::{Ipv4Address, Ipv4Cidr, StaticConfigV4};
            let config = StaticConfigV4 {
                address: Ipv4Cidr::new(Ipv4Address::new(192, 168, 4, 1), 24),
                gateway: None,
                dns_servers: heapless::Vec::new(),
            };
            (interface, embassy_net::Config::ipv4_static(config))
        }
    };

    let resources = RESOURCES.init(StackResources::new());
    let seed = embassy_time::Instant::now().as_ticks();
    let (stack, runner) = embassy_net::new(interface, net_config, resources, seed);
    spawner.spawn(net_task(runner).unwrap());
    stack
}
