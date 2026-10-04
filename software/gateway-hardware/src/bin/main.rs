#![no_std]
#![no_main]

use core::sync::atomic::Ordering;
use embassy_executor::Spawner;
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;
use gateway_hardware::config::LoadOrInit;
use gateway_hardware::ethernet::EthernetPins;
use gateway_hardware::{can, config, console, console_log, ethernet, flash, mqtt, update};

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_hal::main]
async fn main(spawner: Spawner) -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::_160MHz));

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    console_log!("gateway-hardware booting");

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

    // Ethernet is the only network path (ADR 0013). A DHCP client, so
    // nothing needs configuring before the gateway is reachable; the
    // cable can be plugged in at any time.
    let stack = match ethernet::bring_up(EthernetPins {
        eth: peripherals.ETH,
        rxd0: peripherals.GPIO25,
        rxd1: peripherals.GPIO26,
        rx_dv: peripherals.GPIO27,
        txd0: peripherals.GPIO19,
        txd1: peripherals.GPIO22,
        tx_en: peripherals.GPIO21,
        mdc: peripherals.GPIO23,
        mdio: peripherals.GPIO18,
        clock: peripherals.GPIO17,
        reset: peripherals.GPIO5,
    })
    .await
    {
        Some(eth) => Some(ethernet::init_stack(eth, &spawner, cfg.hostname.as_str()).await),
        None => {
            console_log!("ethernet: PHY init failed, running without network");
            None
        }
    };

    if let Some(stack) = stack {
        spawner.spawn(mqtt::mqtt_task(stack).unwrap());
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
