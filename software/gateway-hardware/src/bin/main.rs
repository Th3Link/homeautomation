#![no_std]
#![no_main]

use core::sync::atomic::Ordering;
use embassy_executor::Spawner;
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;
use gateway_hardware::config::LoadOrInit;
use gateway_hardware::ethernet::EthernetPins;
use gateway_hardware::{
    can, cert, config, console, console_log, dhcpv6, ethernet, flash, https, mqtt, update,
    wall_clock, web,
};

esp_bootloader_esp_idf::esp_app_desc!();

/// Called by `esp-backtrace` (feature `custom-halt`) after it has printed a
/// panic: restart instead of halting forever, see
/// `common_hardware::reset`.
#[unsafe(no_mangle)]
fn custom_halt() -> ! {
    common_hardware::reset::restart_after_panic()
}

#[esp_hal::main]
async fn main(spawner: Spawner) -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::_160MHz));

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    // Marks the unused main stack, for `stack_untouched` in `GET /system`.
    gateway_hardware::stack::paint();
    console_log!("gateway-hardware booting");
    // `CoreSw` after a panic restart (see `custom_halt`) as well as after a
    // deliberate reboot; a stream of them without a reboot request is a
    // panic loop.
    console_log!("reset reason: {:?}", esp_hal::system::reset_reason());

    // The heap lives in the RAM the ESP32's ROM leaves free above the
    // normal data segment (`dram2`, ~96 KB), so it costs the statics nothing.
    // It holds the web server's tasks (see `web`), the one MbedTLS session
    // of HTTPS/EST (see `https`) and the RSA signature check of the MQTT
    // TLS; everything else stays on heapless/static allocation.
    esp_alloc::heap_allocator!(#[unsafe(link_section = ".dram2_uninit")] size: 94 * 1024);

    // TLS needs unpredictable random numbers; without a radio the hardware
    // generator only gets entropy from the ADC noise source.
    gateway_hardware::tls::init_entropy(peripherals.RNG, peripherals.ADC1);

    flash::init(peripherals.FLASH);
    config::init().await;
    let cfg = config::config().await.load_or_init().await;

    can::DEVICE_ID.store(cfg.can_id, Ordering::Relaxed);
    can::DEVICE_TYPE.store(cfg.can_type, Ordering::Relaxed);

    can::init(
        peripherals.TWAI0,
        peripherals.GPIO14,
        peripherals.GPIO13,
        &spawner,
    )
    .await;

    // Commissioning console, up regardless of network state — GPIO3/GPIO1
    // (ESP32 default UART0 pins) aren't used by anything else.
    console::init(
        peripherals.UART0,
        peripherals.GPIO3,
        peripherals.GPIO1,
        &spawner,
    )
    .await;

    update::init().await;

    // The HTTPS key and certificate (made on first boot), before the web
    // server needs them.
    https::init();
    cert::init().await;

    // Ethernet is the only network path (ADR 0013). A DHCP client, so
    // nothing needs configuring before the gateway is reachable; the
    // cable can be plugged in at any time.
    let clock_gpio = config::eth_clock_gpio().await;
    let stack = match ethernet::bring_up(
        EthernetPins {
            eth: peripherals.ETH,
            rxd0: peripherals.GPIO25,
            rxd1: peripherals.GPIO26,
            rx_dv: peripherals.GPIO27,
            txd0: peripherals.GPIO19,
            txd1: peripherals.GPIO22,
            tx_en: peripherals.GPIO21,
            mdc: peripherals.GPIO23,
            mdio: peripherals.GPIO18,
            clock16: peripherals.GPIO16,
            clock17: peripherals.GPIO17,
            reset: peripherals.GPIO5,
        },
        clock_gpio,
    )
    .await
    {
        Some(eth) => Some(ethernet::init_stack(eth, &spawner, cfg.hostname.as_str()).await),
        None => {
            console_log!("ethernet: PHY init failed, running without network");
            None
        }
    };

    if let Some(stack) = stack {
        // One session per broker slot; a slot that isn't enabled idles.
        for slot in 0..gateway_core::config::MQTT_BROKERS {
            spawner.spawn(mqtt::mqtt_task(stack, slot).unwrap());
        }
        spawner.spawn(dhcpv6::dhcpv6_task(stack).unwrap());
        spawner.spawn(wall_clock::sntp_task(stack).unwrap());
        spawner.spawn(cert::manager_task(stack).unwrap());
        web::spawn(&spawner, stack, cert::https_enabled().await);
    }

    can::send_can_message(
        gateway_core::can_message_type::CanMessageType::Available,
        &[1],
        false,
    )
    .await;

    loop {
        embassy_time::Timer::after(embassy_time::Duration::from_millis(3_000)).await;
    }
}
