#![no_std]
#![no_main]

use cancomponents::can;
use cancomponents::config;
use cancomponents::device;
use cancomponents::extension::Extension;
use cancomponents::extension::ExtensionType;
use cancomponents::relais::Relais;
use cancomponents::update;
use embassy_executor::Spawner;
use embassy_time::Duration;
use embassy_time::Timer;
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;
use esp_hal_embassy::main;

#[main]
async fn main(spawner: Spawner) -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::_80MHz));

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_hal_embassy::init(timg0.timer0);

    config::init().await;
    device::init().await;
    update::init().await;

    can::init(
        peripherals.TWAI0,
        peripherals.GPIO14,
        peripherals.GPIO13,
        &spawner,
    )
    .await;

    Relais::init(
        peripherals.I2C0,
        peripherals.GPIO21,
        peripherals.GPIO19,
        &spawner,
    );

    Extension::init(
        ExtensionType::GpioInput4,
        peripherals.GPIO15,
        peripherals.GPIO16,
        peripherals.GPIO17,
        peripherals.GPIO18,
        &spawner,
    );

    loop {
        // let frame = block!(twai.receive()).unwrap();
        // println!("Bla");
        Timer::after(Duration::from_millis(3_000)).await;
    }
}

// // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-beta.0/examples/src/bin
