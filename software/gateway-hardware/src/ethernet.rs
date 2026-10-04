//! Wired Ethernet bring-up (the ESP32's built-in EMAC over RMII to a
//! Clause-22 PHY) — mirrors `LAN.cpp`. This is the gateway's **only**
//! network path (it's installed hardwired next to the CAN bus wiring; see
//! ADR 0013 for why WiFi was dropped).
//!
//! The network needs no setup: [`init_stack`] runs dual-stack — a DHCPv4
//! client plus IPv6 SLAAC — so a freshly-flashed gateway just picks up
//! addresses from the LAN (announcing its configured hostname over DHCP,
//! so it's findable by name) and everything else — MQTT broker, credentials — is configured over the
//! serial console. There's deliberately no link-up wait or timeout either:
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
//! - RMII reference clock: `clock_mode = EMAC_CLK_OUT` with
//!   `clock_gpio = EMAC_CLK_OUT_180_GPIO` — the ESP32 generates the 50MHz
//!   RMII clock itself (via APLL) and outputs it on **GPIO17** (the "180"
//!   variant; GPIO16 is the non-inverted `EMAC_CLK_OUT` alternative). To
//!   switch, change [`EthernetPins::clock`]'s type here and the
//!   corresponding `peripherals.GPIO17`/`GPIO16` passed to it in
//!   `main.rs` — they're the only two wired options on ESP32.

use crate::console_log;
use embassy_executor::Spawner;
use embassy_net::{Runner, Stack, StackResources};
use embassy_time::{Duration, Timer};
use esp_hal::ethernet::clock::ApllClock;
use esp_hal::ethernet::phy::generic::GenericPhy;
use esp_hal::ethernet::{Ethernet, EthernetDmaStorage, RmiiPinBundle};
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::peripherals::ETH;
use esp_hal::Async;
use static_cell::StaticCell;

/// `smi_mdc_gpio_num`/`smi_mdio_gpio_num` from `LAN.cpp`.
const PHY_ADDRESS: u8 = 1;

/// DMA descriptor/buffer ring sizes — small is fine for this gateway's
/// traffic volume (MQTT + the occasional web UI request, not bulk transfer).
const RX_DESCRIPTORS: usize = 4;
const TX_DESCRIPTORS: usize = 4;

type EthDriver = Ethernet<'static, Async, GenericPhy>;

static DMA_STORAGE: StaticCell<EthernetDmaStorage<RX_DESCRIPTORS, TX_DESCRIPTORS>> =
    StaticCell::new();
static NET_RESOURCES: StaticCell<StackResources<4>> = StaticCell::new();

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
    /// The RMII reference clock output pin — GPIO17 to match `LAN.cpp`
    /// (`EMAC_CLK_OUT_180_GPIO`), or GPIO16 for the non-inverted
    /// `EMAC_CLK_OUT` alternative some boards use instead.
    pub clock: esp_hal::peripherals::GPIO17<'static>,
    /// PHY reset, active-low (`reset_gpio_num = 5` in `LAN.cpp`).
    pub reset: esp_hal::peripherals::GPIO5<'static>,
}

/// Pulses the PHY's reset line and brings up the EMAC over RMII. Returns
/// `None` only if the EMAC/PHY itself fails to initialize (no PHY answering
/// on the MDIO bus — a hardware fault, not a missing cable).
pub async fn bring_up(pins: EthernetPins) -> Option<EthDriver> {
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

    let storage = DMA_STORAGE.init(EthernetDmaStorage::new());
    let rmii_pins = RmiiPinBundle {
        clock: ApllClock::new(pins.clock),
        rxd0: pins.rxd0,
        rxd1: pins.rxd1,
        rx_dv: pins.rx_dv,
        txd0: pins.txd0,
        txd1: pins.txd1,
        tx_en: pins.tx_en,
        mdc: pins.mdc,
        mdio: pins.mdio,
    };

    match Ethernet::new(
        pins.eth,
        storage,
        mac_addr,
        GenericPhy::new(PHY_ADDRESS),
        rmii_pins,
    ) {
        Ok(eth) => Some(eth.into_async()),
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

/// Starts the `embassy-net` stack over `eth` — a DHCPv4 client, matching
/// `LAN.cpp`'s (ESP-IDF default) behavior, announcing `hostname` (DHCP
/// option 12; silently omitted if it doesn't fit the 32-byte limit), plus
/// IPv6 SLAAC (smoltcp has no DHCPv6 client).
pub async fn init_stack(eth: EthDriver, spawner: &Spawner, hostname: &str) -> Stack<'static> {
    let mut dhcp = embassy_net::DhcpConfig::default();
    dhcp.hostname = heapless::String::try_from(hostname).ok();

    let resources = NET_RESOURCES.init(StackResources::new());
    let seed = embassy_time::Instant::now().as_ticks();
    // `Config` is #[non_exhaustive]: start from the DHCPv4 constructor and
    // add SLAAC on top (dual-stack).
    let mut config = embassy_net::Config::dhcpv4(dhcp);
    config.ipv6 = embassy_net::ConfigV6::Slaac;
    let (stack, runner) = embassy_net::new(eth, config, resources, seed);
    spawner.spawn(net_task(runner).unwrap());
    stack
}
