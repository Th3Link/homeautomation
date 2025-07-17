use embassy_executor::Spawner;
use esp_hal::gpio::Input;
use esp_hal::gpio::InputConfig;
use esp_hal::gpio::InputPin;
use esp_hal::gpio::Pull;

pub enum ExtensionType {
    GpioInput4,
    ButtonBoardI2c,
    ButtonBoardGpio,
    PwmBoard,
    SensorBoard,
}

pub struct Extension {}

impl Extension {
    pub fn init(
        ext_type: ExtensionType,
        gpio0: impl InputPin,
        gpio1: impl InputPin,
        gpio2: impl InputPin,
        gpio3: impl InputPin,
        _spawner: &Spawner,
    ) -> Self {
        match ext_type {
            ExtensionType::GpioInput4 => {
                // Alle 4 GPIOs als Input konfigurieren
                // evtl. Interrupts aktivieren
                // spawn InputTask
                let config = InputConfig::default().with_pull(Pull::Up);
                let pin0 = Input::new(gpio0, config);
                let pin1 = Input::new(gpio1, config);
                let pin2 = Input::new(gpio2, config);
                let pin3 = Input::new(gpio3, config);

                let _gpios = [pin0, pin1, pin2, pin3];

                let ext = Extension {};

                //_spawner.spawn(Self::gpio_input_task(gpios)).unwrap();

                return ext;
            }
            ExtensionType::ButtonBoardI2c => {
                // I2C mit SDA/SCL initialisieren
                // z. B. über gpio0/gpio1
                // spawn ButtonPollTask
            }
            ExtensionType::ButtonBoardGpio => {
                // 4 GPIOs lesen
            }
            ExtensionType::PwmBoard => {
                // PWM-I2C initialisieren
            }
            ExtensionType::SensorBoard => {
                // Kombination aus I2C + GPIO
            }
        }

        Extension {}
    }
}
