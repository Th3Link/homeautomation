//! Wired Ethernet bring-up (the ESP32's built-in EMAC over RMII to a
//! Clause-22 PHY) — mirrors `LAN.cpp`. This is the gateway's **only**
//! network path (it's installed hardwired next to the CAN bus wiring; see
//! ADR 0013 for why WiFi was dropped).
//!
//! The network needs no setup: by default [`init_stack`] runs dual-stack —
//! a DHCPv4 client plus IPv6 SLAAC, and DHCPv6 where the routers ask for
//! it ([`crate::dhcpv6`]) — so a freshly-flashed gateway just picks up
//! addresses from the LAN (announcing its configured hostname over DHCP,
//! so it's findable by name) and everything else — MQTT broker, credentials — is configured over the
//! serial console. The network settings ([`gateway_core::network`]) can
//! replace DHCP with a link-local or a fixed address, switch SLAAC and
//! DHCPv6 off or DHCPv6 on for good, and add an IPv6 address by hand; they
//! are read at boot. There's deliberately no link-up wait or timeout either:
//! the EMAC driver reports link state to `embassy-net`, which starts DHCP
//! when the cable comes up and redoes it if the link drops, so plugging
//! the cable in late (or swapping it) just works.
//!
//! Pin wiring matches `LAN.cpp`'s `eth_esp32_emac_config_t` exactly: the
//! RMII data pins (RXD0/RXD1/CRS_DV/TXD0/TXD1/TX_EN) are fixed by the ESP32
//! silicon itself, not a board choice. What *is* board-specific, read
//! straight from that config:
//! - MDC = GPIO23, MDIO = GPIO18 (`smi_mdc_gpio_num`/`smi_mdio_gpio_num`)
//! - PHY address = 1, fixed, not auto-discovered (`phy_addr = 1`)
//! - PHY reset = GPIO5 (`reset_gpio_num`), driven manually here since
//!   esp-hal's `Phy` trait has no reset-pin concept of its own
//! - RMII reference clock: the ESP32 generates the 50MHz RMII clock itself
//!   (via APLL) and outputs it on **GPIO16** or **GPIO17** — the only two
//!   pins the silicon offers for it. Which one is right depends on the
//!   board revision: the rev1 boards (and the original `LAN.cpp`,
//!   `EMAC_CLK_OUT_180_GPIO`) use **GPIO17**; the rev2 schematic routes IO16
//!   to the PHY's clock input instead. It is a console setting
//!   (`eth-clock`, default 17), read at boot. With the wrong pin the PHY
//!   doesn't answer on MDIO and bring-up fails with `Phy(Timeout)`.

use crate::console_log;
use core::sync::atomic::{AtomicBool, Ordering};
use embassy_executor::Spawner;
use embassy_net::{
    Dhcpv6Lease, Ipv4Cidr, Ipv6Cidr, Ipv6RouterFlags, ManualConfigV6, Runner, Stack,
    StackResources, StaticConfigV4, StaticConfigV6,
};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_sync::watch::Watch;
use embassy_time::{Duration, Instant, Timer};
use esp_hal::ethernet::clock::ApllClock;
use esp_hal::ethernet::phy::generic::GenericPhy;
use esp_hal::ethernet::{Ethernet, EthernetDmaStorage, RmiiPinBundle};
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::peripherals::ETH;
use esp_hal::Async;
use gateway_core::network::{link_local_v4, Ipv4Mode, NetworkConfig};
use static_cell::{ConstStaticCell, StaticCell};

/// `smi_mdc_gpio_num`/`smi_mdio_gpio_num` from `LAN.cpp`.
const PHY_ADDRESS: u8 = 1;

/// DMA descriptor/buffer ring sizes — small is fine for this gateway's
/// traffic volume (MQTT + the occasional web UI request, not bulk transfer).
const RX_DESCRIPTORS: usize = 8;
const TX_DESCRIPTORS: usize = 4;

type RawDriver = Ethernet<'static, Async, GenericPhy>;
type EthDriver = SoftwareChecksums<RawDriver>;

/// Passes everything through to the Ethernet driver, but has the network
/// stack compute and check all checksums itself.
///
/// The MAC's checksum insertion gets a good part of them wrong (and writes
/// into the addresses of IPv6 frames); the patched esp-hal (a fork, see
/// `software/patches/README.md`) switches it off, which makes the stack
/// fill them in.
/// This also has the stack check received ones, rather than trusting the
/// same MAC's receive checksum engine alone.
pub struct SoftwareChecksums<D>(D);

impl<D: embassy_net_driver::Driver> embassy_net_driver::Driver for SoftwareChecksums<D> {
    type RxToken<'a>
        = D::RxToken<'a>
    where
        Self: 'a;
    type TxToken<'a>
        = D::TxToken<'a>
    where
        Self: 'a;

    fn receive(
        &mut self,
        cx: &mut core::task::Context<'_>,
    ) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        self.0.receive(cx)
    }

    fn transmit(&mut self, cx: &mut core::task::Context<'_>) -> Option<Self::TxToken<'_>> {
        self.0.transmit(cx)
    }

    fn link_state(&mut self, cx: &mut core::task::Context<'_>) -> embassy_net_driver::LinkState {
        self.0.link_state(cx)
    }

    fn capabilities(&self) -> embassy_net_driver::Capabilities {
        let mut capabilities = self.0.capabilities();
        capabilities.checksum.ipv4 = embassy_net_driver::Checksum::Both;
        capabilities.checksum.tcp = embassy_net_driver::Checksum::Both;
        capabilities.checksum.udp = embassy_net_driver::Checksum::Both;
        capabilities.checksum.icmpv4 = embassy_net_driver::Checksum::Both;
        capabilities.checksum.icmpv6 = embassy_net_driver::Checksum::Both;
        capabilities
    }

    fn hardware_address(&self) -> embassy_net_driver::HardwareAddress {
        self.0.hardware_address()
    }
}

// Constructed in place: by value it would pass through the stack.
static DMA_STORAGE: ConstStaticCell<EthernetDmaStorage<RX_DESCRIPTORS, TX_DESCRIPTORS>> =
    ConstStaticCell::new(EthernetDmaStorage::new());
// DHCP, DHCPv6, DNS, two MQTT brokers, six web sockets (five HTTPS and the
// redirect on port 80; plain HTTP uses three), the EST client and SNTP, plus
// one spare. Keep in step with `web::HTTPS_TASKS`.
static NET_RESOURCES: StaticCell<StackResources<14>> = StaticCell::new();

/// A look at the network state, for the console's `network`.
#[derive(Debug, Clone)]
pub struct NetInfo {
    pub link_up: bool,
    pub mac: [u8; 6],
    pub v4: Option<StaticConfigV4>,
    /// The first SLAAC address, the router and the RDNSS DNS servers.
    pub v6: Option<StaticConfigV6>,
    /// All IPv6 addresses: link-local, SLAAC (one per prefix), DHCPv6.
    pub v6_addresses: heapless::Vec<Ipv6Cidr, 5>,
    pub router_flags: Ipv6RouterFlags,
    pub dhcpv6: Option<Dhcpv6Lease>,
    /// The settings the network was started with.
    pub settings: NetworkConfig,
}

impl NetInfo {
    /// Where an IPv6 address of [`Self::v6_addresses`] comes from:
    /// `link-local`, `manual`, `DHCPv6` or `SLAAC`.
    pub fn v6_origin(&self, cidr: &Ipv6Cidr) -> &'static str {
        if cidr.address().is_unicast_link_local() {
            "link-local"
        } else if self.settings.ipv6_address == Some(cidr.address()) {
            "manual"
        } else if self.dhcpv6.as_ref().and_then(|lease| lease.address) == Some(*cidr) {
            "DHCPv6"
        } else {
            "SLAAC"
        }
    }

    /// Where the IPv4 address comes from: `DHCP`, `link-local` or `manual`.
    pub fn v4_origin(&self) -> &'static str {
        match self.settings.ipv4_mode {
            Ipv4Mode::Dhcp => "DHCP",
            Ipv4Mode::LinkLocal => "link-local",
            Ipv4Mode::Manual => "manual",
        }
    }

    /// Where the IPv4 DNS servers come from: by hand, or the DHCP server.
    pub fn v4_dns_origin(&self) -> &'static str {
        if self.settings.dns_servers.is_empty() {
            "DHCPv4"
        } else {
            "manual"
        }
    }

    /// The IPv6 default gateway: the router SLAAC took, or without SLAAC
    /// the one set by hand.
    pub fn v6_gateway(&self) -> Option<embassy_net::Ipv6Address> {
        if self.settings.slaac {
            self.v6.as_ref().and_then(|v6| v6.gateway)
        } else {
            self.settings.ipv6_gateway
        }
    }
}

/// The stack lives in the network tasks (it can't be shared between
/// tasks), so others ask [`info_task`] for a [`NetInfo`].
static STARTED: AtomicBool = AtomicBool::new(false);
static INFO_REQUEST: Signal<CriticalSectionRawMutex, ()> = Signal::new();
static INFO: Signal<CriticalSectionRawMutex, NetInfo> = Signal::new();

/// The current network state; `None` if the network isn't started.
pub async fn info() -> Option<NetInfo> {
    if !STARTED.load(Ordering::Relaxed) {
        return None;
    }
    INFO.reset();
    INFO_REQUEST.signal(());
    embassy_time::with_timeout(Duration::from_secs(1), INFO.wait())
        .await
        .ok()
}

#[embassy_executor::task]
async fn info_task(stack: Stack<'static>, settings: NetworkConfig) {
    loop {
        INFO_REQUEST.wait().await;
        let mac = match stack.hardware_address() {
            embassy_net::HardwareAddress::Ethernet(mac) => mac.0,
        };
        INFO.signal(NetInfo {
            link_up: stack.is_link_up(),
            mac,
            v4: stack.config_v4(),
            v6: stack.config_v6(),
            v6_addresses: stack.ipv6_addresses().into_iter().take(5).collect(),
            router_flags: stack.ipv6_router_flags(),
            dhcpv6: stack.dhcpv6_lease(),
            settings: settings.clone(),
        });
    }
}

/// Peripherals/pins Ethernet needs from `main.rs` — grouped since there are
/// nine of them and they're always passed together.
pub struct EthernetPins {
    pub eth: ETH<'static>,
    pub rxd0: esp_hal::peripherals::GPIO25<'static>,
    pub rxd1: esp_hal::peripherals::GPIO26<'static>,
    pub rx_dv: esp_hal::peripherals::GPIO27<'static>,
    pub txd0: esp_hal::peripherals::GPIO19<'static>,
    pub txd1: esp_hal::peripherals::GPIO22<'static>,
    pub tx_en: esp_hal::peripherals::GPIO21<'static>,
    pub mdc: esp_hal::peripherals::GPIO23<'static>,
    pub mdio: esp_hal::peripherals::GPIO18<'static>,
    /// The two pins the RMII reference clock can come out on; which one is
    /// used is the `clock_gpio` argument of [`bring_up`].
    pub clock16: esp_hal::peripherals::GPIO16<'static>,
    pub clock17: esp_hal::peripherals::GPIO17<'static>,
    /// PHY reset, active-low (`reset_gpio_num = 5` in `LAN.cpp`).
    pub reset: esp_hal::peripherals::GPIO5<'static>,
}

/// Pulses the PHY's reset line and brings up the EMAC over RMII. Returns
/// `None` only if the EMAC/PHY itself fails to initialize (no PHY answering
/// on the MDIO bus — a hardware fault, not a missing cable).
pub async fn bring_up(pins: EthernetPins, clock_gpio: u8) -> Option<EthDriver> {
    // Active-low reset: hold low, then release and let the PHY's internal
    // reset sequence settle before any MDIO access (Ethernet::new() does
    // phy.init() internally, so this must happen first).
    let mut reset = Output::new(pins.reset, Level::Low, OutputConfig::default());
    Timer::after(Duration::from_millis(10)).await;
    reset.set_high();
    Timer::after(Duration::from_millis(10)).await;
    // `Output` doesn't reset the pin on drop (it has no `Drop` impl), so
    // simply letting `reset` go out of scope here already leaves the pin
    // driven high for the rest of the device's lifetime.

    let mac_addr: [u8; 6] = esp_hal::efuse::base_mac_address()
        .as_bytes()
        .try_into()
        .expect("MAC address is always 6 bytes");

    let storage = DMA_STORAGE.take();
    console_log!("ethernet: RMII clock on GPIO{clock_gpio}");
    let phy = GenericPhy::new(PHY_ADDRESS);
    // The clock pin's type is part of the pin bundle's type, so the choice
    // between the two is made by building the bundle in each branch.
    macro_rules! rmii_pins {
        ($clock:expr) => {
            RmiiPinBundle {
                clock: ApllClock::new($clock),
                rxd0: pins.rxd0,
                rxd1: pins.rxd1,
                rx_dv: pins.rx_dv,
                txd0: pins.txd0,
                txd1: pins.txd1,
                tx_en: pins.tx_en,
                mdc: pins.mdc,
                mdio: pins.mdio,
            }
        };
    }
    let started = if clock_gpio == 17 {
        Ethernet::new(pins.eth, storage, mac_addr, phy, rmii_pins!(pins.clock17))
    } else {
        Ethernet::new(pins.eth, storage, mac_addr, phy, rmii_pins!(pins.clock16))
    };

    match started {
        Ok(eth) => Some(SoftwareChecksums(eth.into_async())),
        Err(e) => {
            console_log!("ethernet: init failed: {e:?}");
            None
        }
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, EthDriver>) {
    runner.run().await
}

/// Starts the `embassy-net` stack over `eth` with `settings`. By default
/// a DHCPv4 client, matching `LAN.cpp`'s (ESP-IDF default) behavior,
/// announcing `hostname` (DHCP option 12; silently omitted if it doesn't
/// fit the 32-byte limit), plus IPv6 SLAAC (DHCPv6 runs in its own task,
/// [`crate::dhcpv6`]).
pub async fn init_stack(
    eth: EthDriver,
    spawner: &Spawner,
    hostname: &str,
    settings: NetworkConfig,
) -> Stack<'static> {
    let mac = match embassy_net_driver::Driver::hardware_address(&eth) {
        embassy_net_driver::HardwareAddress::Ethernet(mac) => mac,
        _ => [0; 6],
    };
    let dns_servers = settings.dns_servers.iter().copied().collect();
    let ipv4 = match settings.ipv4_mode {
        Ipv4Mode::Dhcp => {
            let mut dhcp = embassy_net::DhcpConfig::default();
            dhcp.hostname = heapless::String::try_from(hostname).ok();
            // The first DISCOVER often goes out while the switch port is
            // still coming up and is lost; smoltcp's default is to try
            // again only after 10 s, which held up everything that needs
            // the network.
            dhcp.retry_config.discover_timeout = smoltcp::time::Duration::from_secs(2);
            dhcp.retry_config.initial_request_timeout = smoltcp::time::Duration::from_secs(2);
            dhcp.dns_servers = (!settings.dns_servers.is_empty()).then_some(dns_servers);
            embassy_net::ConfigV4::Dhcp(dhcp)
        }
        Ipv4Mode::LinkLocal => embassy_net::ConfigV4::Static(StaticConfigV4 {
            address: Ipv4Cidr::new(link_local_v4(mac), 16),
            gateway: None,
            dns_servers,
        }),
        Ipv4Mode::Manual => embassy_net::ConfigV4::Static(StaticConfigV4 {
            address: Ipv4Cidr::new(
                settings
                    .ipv4_address
                    .unwrap_or(core::net::Ipv4Addr::UNSPECIFIED),
                settings.ipv4_prefix,
            ),
            gateway: settings.ipv4_gateway,
            dns_servers,
        }),
    };

    let resources = NET_RESOURCES.init(StackResources::new());
    let seed = embassy_time::Instant::now().as_ticks();
    // `Config` is #[non_exhaustive]: start from the default and set both
    // families (dual-stack). Without SLAAC smoltcp ignores the Router
    // Advertisements; the link-local address is there all the same.
    let mut config = embassy_net::Config::default();
    config.ipv4 = ipv4;
    config.ipv6 = if settings.slaac {
        embassy_net::ConfigV6::Slaac
    } else {
        embassy_net::ConfigV6::None
    };
    let (stack, runner) = embassy_net::new(eth, config, resources, seed);
    if settings.ipv6_address.is_some() || settings.ipv6_gateway.is_some() {
        stack.set_manual_v6(Some(ManualConfigV6 {
            address: settings
                .ipv6_address
                .map(|address| Ipv6Cidr::new(address, settings.ipv6_prefix)),
            gateway: settings.ipv6_gateway,
        }));
    }
    spawner.spawn(net_task(runner).unwrap());
    spawner.spawn(address_log_task(stack).unwrap());
    spawner.spawn(info_task(stack, settings).unwrap());
    spawner.spawn(ready_task(stack).unwrap());
    STARTED.store(true, Ordering::Relaxed);
    stack
}

/// Whether the network is ready, see [`wait_ready`].
static READY: Watch<CriticalSectionRawMutex, bool, 8> = Watch::new_with(false);
/// Once one IP version is configured, how long to wait for the other: a
/// network may have only one.
const READY_GRACE: Duration = Duration::from_secs(5);

/// Waits until the network is ready: the link is up with a DHCPv4 lease
/// and a global IPv6 address (SLAAC or DHCPv6), or with one of the two
/// and [`READY_GRACE`] has passed without the other. What connects to
/// servers starts here, so that its first attempt doesn't go out before
/// the addresses and routes are there (and then waits for its retry).
pub async fn wait_ready() {
    match READY.receiver() {
        Some(mut receiver) => {
            receiver.get_and(|&ready| ready).await;
        }
        // More waiters than receivers: look now and then.
        None => {
            while READY.try_get() != Some(true) {
                Timer::after(Duration::from_millis(200)).await;
            }
        }
    }
}

fn has_global_ipv6(stack: Stack<'_>) -> bool {
    stack
        .ipv6_addresses()
        .iter()
        .any(|cidr| !cidr.address().is_unicast_link_local())
}

#[embassy_executor::task]
async fn ready_task(stack: Stack<'static>) {
    let sender = READY.sender();
    loop {
        stack.wait_link_up().await;
        // The addresses arrive without an event of their own (SLAAC, the
        // DHCPv6 lease): look for them every 100 ms.
        let mut first = None;
        let (v4, v6) = loop {
            let (v4, v6) = (stack.config_v4().is_some(), has_global_ipv6(stack));
            if v4 && v6 {
                break (v4, v6);
            }
            if v4 || v6 {
                let since = *first.get_or_insert_with(Instant::now);
                if since.elapsed() >= READY_GRACE {
                    break (v4, v6);
                }
            }
            if !stack.is_link_up() {
                first = None;
            }
            Timer::after(Duration::from_millis(100)).await;
        };
        let uptime = Instant::now().as_millis();
        console_log!(
            "network: ready after {}.{} s ({})",
            uptime / 1000,
            uptime % 1000 / 100,
            match (v4, v6) {
                (true, true) => "IPv4 and IPv6",
                (true, false) => "IPv4 only",
                _ => "IPv6 only",
            }
        );
        sender.send(true);
        embassy_futures::select::select(stack.wait_link_down(), stack.wait_config_down()).await;
        sender.send(false);
        console_log!("network: not ready");
    }
}

/// Logs when the DHCP lease arrives or is lost — the only visible sign that
/// the wired network really works end to end (a PHY can report "link up"
/// over MDIO even if the RMII clock or data lines are wrong).
#[embassy_executor::task]
async fn address_log_task(stack: Stack<'static>) {
    loop {
        stack.wait_config_up().await;
        match stack.config_v4() {
            Some(config) => console_log!(
                "ethernet: address {} gateway {:?}",
                config.address,
                config.gateway
            ),
            None => console_log!("ethernet: network configured"),
        }
        stack.wait_config_down().await;
        console_log!("ethernet: address lost");
    }
}
