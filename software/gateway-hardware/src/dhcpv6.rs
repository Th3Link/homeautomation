//! DHCPv6 (the `dhcpeav6-client` crate of dhcpea) on the gateway's network stack.
//!
//! The routers' Router Advertisements decide what it does: with `M` set it
//! leases an address (stateful), with only `O` it fetches the other
//! configuration (stateless), with neither it stays quiet. The address and
//! the DNS servers go to `embassy-net` next to SLAAC's
//! ([`Stack::set_dhcpv6_lease`]); the NTP server is what SNTP uses unless
//! one is configured ([`crate::cert::ntp_server`]). Before a restart the
//! address is given back ([`release`]).

use crate::console_log;
use core::cell::RefCell;
use core::fmt::Write;
use core::sync::atomic::{AtomicBool, Ordering};
use dhcpeav6_client::{
    Client, ClientConfig, Configuration, Duid, Mode, State, ALL_DHCP_RELAY_AGENTS_AND_SERVERS,
    CLIENT_PORT, SERVER_PORT,
};
use embassy_futures::select::{select, select4, Either, Either4};
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::{Dhcpv6Lease, IpEndpoint, Ipv6Cidr, Stack};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant, Timer};

/// `embassy-net` takes one DHCPv6 address.
const ADDRESSES: usize = 1;
/// DNS and NTP servers kept (each); `embassy-net` takes three DNS servers.
const SERVERS: usize = 3;
/// Bytes of domain names kept (each list: search domains, NTP server names).
const NAME_BYTES: usize = 128;

type Dhcpv6Client = Client<ADDRESSES, SERVERS, NAME_BYTES>;
pub type Dhcpv6Configuration = Configuration<ADDRESSES, SERVERS, NAME_BYTES>;

/// What the client is doing and has, for the console.
#[derive(Debug, Clone)]
pub struct Status {
    pub mode: Mode,
    pub state: State,
    pub configuration: Option<Dhcpv6Configuration>,
}

static STATUS: Mutex<CriticalSectionRawMutex, RefCell<Status>> = Mutex::new(RefCell::new(Status {
    mode: Mode::Off,
    state: State::Idle,
    configuration: None,
}));
static RUNNING: AtomicBool = AtomicBool::new(false);
static RELEASE: Signal<CriticalSectionRawMutex, ()> = Signal::new();
static RELEASED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// Runs `f` on the current status (in a critical section: keep it short).
pub fn with_status<R>(f: impl FnOnce(&Status) -> R) -> R {
    STATUS.lock(|status| f(&status.borrow()))
}

/// The NTP server DHCPv6 named, as text: the first address, else the
/// first name.
pub fn ntp_server<const N: usize>() -> Option<heapless::String<N>> {
    with_status(|status| {
        let config = status.configuration.as_ref()?;
        let mut text = heapless::String::new();
        match config.ntp_servers.first() {
            Some(address) => write!(text, "{address}").ok()?,
            None => write!(text, "{}", config.ntp_names.iter().next()?).ok()?,
        }
        Some(text)
    })
}

/// Gives a leased address back before a restart (RFC 8415 § 18.2.7), so
/// that the server can hand it out again at once. Waits for the server's
/// answer, a second at most.
pub async fn release() {
    if !RUNNING.load(Ordering::Relaxed) {
        return;
    }
    RELEASED.reset();
    RELEASE.signal(());
    let _ = embassy_time::with_timeout(Duration::from_secs(1), RELEASED.wait()).await;
}

fn now() -> dhcpeav6_client::Instant {
    dhcpeav6_client::Instant::from_millis(Instant::now().as_millis())
}

#[embassy_executor::task]
pub async fn dhcpv6_task(stack: Stack<'static>) {
    // On the heap, like the web server's tasks: the client and its buffers
    // (~2.5 KB) would otherwise be static and take the stack's room.
    crate::heap::boxed(|| run(stack)).await
}

async fn run(stack: Stack<'static>) {
    let embassy_net::HardwareAddress::Ethernet(mac) = stack.hardware_address();
    let mac = mac.0;
    let rng = esp_hal::rng::Rng::new();
    let mut client = Dhcpv6Client::new(ClientConfig {
        duid: Duid::link_layer(mac),
        // One IA_NA on one interface: any number that stays the same.
        iaid: u32::from_be_bytes([mac[2], mac[3], mac[4], mac[5]]),
        seed: u64::from(rng.random()) << 32 | u64::from(rng.random()),
    });

    let mut rx_meta = [PacketMetadata::EMPTY; 2];
    let mut tx_meta = [PacketMetadata::EMPTY; 2];
    let mut rx = [0u8; 768];
    let mut tx = [0u8; 384];
    let mut socket = UdpSocket::new(stack, &mut rx_meta, &mut rx, &mut tx_meta, &mut tx);
    if socket.bind(CLIENT_PORT).is_err() {
        console_log!("dhcpv6: cannot bind port {CLIENT_PORT}");
        return;
    }
    RUNNING.store(true, Ordering::Relaxed);
    let servers = IpEndpoint::new(ALL_DHCP_RELAY_AGENTS_AND_SERVERS.into(), SERVER_PORT);

    let mut flags = stack.ipv6_router_flags();
    let mut link_up = stack.is_link_up();
    let mut releasing = false;
    let mut message = [0u8; 384];
    let mut published = None;
    client.set_mode(Mode::from_router_flags(flags.managed, flags.other), now());
    loop {
        while let Ok(Some(len)) = client.poll(now(), &mut message) {
            // Without a link the message is lost; the retransmissions go on.
            let _ = socket.send_to(&message[..len], servers).await;
        }
        let changed = client.configuration_changed();
        if changed {
            apply(stack, &client);
        }
        if changed || published != Some((client.mode(), client.state())) {
            published = Some((client.mode(), client.state()));
            STATUS.lock(|status| {
                *status.borrow_mut() = Status {
                    mode: client.mode(),
                    state: client.state(),
                    configuration: client.configuration().cloned(),
                }
            });
        }
        if releasing && client.state() == State::Idle {
            RELEASED.signal(());
            releasing = false;
        }

        let deadline = client
            .poll_at()
            .map_or(Instant::MAX, |at| Instant::from_millis(at.as_millis()));
        let link_change = async {
            if link_up {
                stack.wait_link_down().await
            } else {
                stack.wait_link_up().await
            }
        };
        let event = select4(
            socket.recv_from_with(|data, _| client.receive(now(), data)),
            Timer::at(deadline),
            stack.wait_ipv6_router_flags_change(flags),
            select(link_change, RELEASE.wait()),
        )
        .await;
        match event {
            Either4::First(()) | Either4::Second(()) => {}
            Either4::Third(()) => {
                flags = stack.ipv6_router_flags();
                if !releasing {
                    client.set_mode(Mode::from_router_flags(flags.managed, flags.other), now());
                }
            }
            Either4::Fourth(Either::First(())) => {
                link_up = !link_up;
                if link_up {
                    client.link_reconnected(now());
                }
            }
            Either4::Fourth(Either::Second(())) => {
                client.release(now());
                releasing = true;
            }
        }
    }
}

/// Hands what the client has to `embassy-net`.
fn apply(stack: Stack<'_>, client: &Dhcpv6Client) {
    let config = client.configuration();
    let address = config.and_then(|c| c.addresses.first());
    match (config, address) {
        (Some(config), Some(address)) => console_log!(
            "dhcpv6: address {} from {}",
            address.address,
            config.server_id
        ),
        (Some(config), None) => console_log!("dhcpv6: configuration from {}", config.server_id),
        (None, _) => console_log!("dhcpv6: no configuration"),
    }
    stack.set_dhcpv6_lease(config.map(|config| Dhcpv6Lease {
        address: address.map(|a| Ipv6Cidr::new(a.address, 128)),
        dns_servers: config.dns_servers.iter().copied().take(3).collect(),
    }));
}
