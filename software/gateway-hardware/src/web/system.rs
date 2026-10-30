//! `GET /api/v1/system`.

use super::error::ApiError;
use super::json::json;
use core::fmt::Write;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use heapless::{String, Vec};
use picoserve::response::{IntoResponse, StatusCode};
use serde::Serialize;

static RESTART: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// Restarts the gateway a moment after [`restart`] answered, so that the
/// answer still gets out.
#[embassy_executor::task]
pub async fn restart_task() {
    RESTART.wait().await;
    Timer::after(Duration::from_millis(500)).await;
    crate::dhcpv6::release().await;
    esp_hal::system::software_reset();
}

/// Restarts the gateway shortly.
pub fn request_restart() {
    RESTART.signal(());
}

pub async fn restart() -> impl IntoResponse {
    request_restart();
    StatusCode::ACCEPTED
}

/// An address and where it comes from.
#[derive(Serialize)]
struct Origin {
    address: String<48>,
    origin: &'static str,
}

fn text<const N: usize>(value: impl core::fmt::Display) -> String<N> {
    let mut text = String::new();
    let _ = write!(text, "{value}");
    text
}

/// The network as the console's `network` shows it.
#[derive(Serialize)]
struct Network {
    link_up: bool,
    mac: String<17>,
    /// With prefix length, e.g. `192.168.1.20/24`.
    ipv4: Option<String<18>>,
    ipv4_gateway: Option<String<15>>,
    /// Link-local first; origin `link-local`, `SLAAC` or `DHCPv6`.
    ipv6: Vec<Origin, 5>,
    ipv6_gateway: Option<String<39>>,
    /// Origin `DHCPv4`, `RDNSS` or `DHCPv6`.
    dns_servers: Vec<Origin, 9>,
    /// What the routers' Router Advertisements offer: `M` addresses,
    /// `O` other configuration over DHCPv6.
    ipv6_managed: bool,
    ipv6_other: bool,
    /// `off`, `stateless` or `stateful`.
    dhcpv6_mode: &'static str,
}

async fn network() -> Option<Network> {
    let net = crate::ethernet::info().await?;
    let m = net.mac;
    let mut dns_servers = Vec::new();
    let v4_dns = net.v4.iter().flat_map(|v4| v4.dns_servers.iter());
    for server in v4_dns {
        let _ = dns_servers.push(Origin {
            address: text(server),
            origin: "DHCPv4",
        });
    }
    for server in net.v6.iter().flat_map(|v6| v6.dns_servers.iter()) {
        let _ = dns_servers.push(Origin {
            address: text(server),
            origin: "RDNSS",
        });
    }
    for server in net.dhcpv6.iter().flat_map(|lease| lease.dns_servers.iter()) {
        let _ = dns_servers.push(Origin {
            address: text(server),
            origin: "DHCPv6",
        });
    }
    Some(Network {
        link_up: net.link_up,
        mac: text(format_args!(
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            m[0], m[1], m[2], m[3], m[4], m[5]
        )),
        ipv4: net.v4.as_ref().map(|v4| text(v4.address)),
        ipv4_gateway: net.v4.as_ref().and_then(|v4| v4.gateway).map(text),
        ipv6: net
            .v6_addresses
            .iter()
            .map(|cidr| Origin {
                address: text(cidr),
                origin: net.v6_origin(cidr),
            })
            .collect(),
        ipv6_gateway: net.v6.as_ref().and_then(|v6| v6.gateway).map(text),
        dns_servers,
        ipv6_managed: net.router_flags.managed,
        ipv6_other: net.router_flags.other,
        dhcpv6_mode: crate::dhcpv6::with_status(|status| match status.mode {
            dhcpeav6_client::Mode::Off => "off",
            dhcpeav6_client::Mode::Stateless => "stateless",
            dhcpeav6_client::Mode::Stateful => "stateful",
        }),
    })
}

#[derive(Serialize)]
pub struct System {
    firmware_version: &'static str,
    uptime_s: u64,
    heap_used: usize,
    heap_free: usize,
    /// Main stack bytes never used since boot: the margin left.
    stack_untouched: usize,
    stack_size: usize,
    /// `null` without Ethernet.
    network: Option<Network>,
}

pub async fn get_system() -> Result<impl IntoResponse, ApiError> {
    // On the heap: the network part makes it ~1 KB (see `crate::heap`).
    crate::heap::boxed(|| async {
        json(&System {
            firmware_version: env!("VERGEN_GIT_DESCRIBE"),
            uptime_s: embassy_time::Instant::now().as_secs(),
            heap_used: esp_alloc::HEAP.used(),
            heap_free: esp_alloc::HEAP.free(),
            stack_untouched: crate::stack::untouched(),
            stack_size: crate::stack::size(),
            network: network().await,
        })
        .await
    })
    .await
}
