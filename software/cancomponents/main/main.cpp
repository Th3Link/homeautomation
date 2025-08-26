/*
 * CAN Lightswitch module
 */

#include <cstdio>
#include <cstdlib>
#include <cstring>

#include <esp_err.h>
#include <esp_log.h>
#include <freertos/FreeRTOS.h>
#include <freertos/semphr.h>
#include <freertos/task.h>

#include "ConsoleCommandDevice.hpp"
#include "ExtensionBoard.hpp"
#include "Light.hpp"
#include "Relais.hpp"
#include "Selftest.hpp"
#include "esp32-ha-lib/AmbientLightSensor.hpp"
#include "esp32-ha-lib/Button.hpp"
#include "esp32-ha-lib/CAN.hpp"
#include "esp32-ha-lib/Console.hpp"
#include "esp32-ha-lib/Device.hpp"
#include "esp32-ha-lib/EEPROM.hpp"
#include "esp32-ha-lib/I2C.hpp"
#include "esp32-ha-lib/Nightlight.hpp"
#include "esp32-ha-lib/PinConfig.hpp"
#include "esp32-ha-lib/PresenceSensor.hpp"
#include "esp32-ha-lib/THSensor.hpp"
#include "esp32-ha-lib/Update.hpp"
/* --------------------- Definitions and static variables ------------------ */

#define TAG "CANLIGHTSWITCH"

static SemaphoreHandle_t shutdown_sem;

/* --------------------------- Tasks and Functions -------------------------- */

static PinConfig pin_config;
static CAN can;
static Update update(can);
static Device device(can);
static I2C i2c;
static Light light(can);
static THSensor ext_thsensor(can);
static THSensor thsensor(can);
static AmbientLightSensor ambient_light_sensor(can);
static Nightlight nightlight(can);
static PresenceSensor presence_sensor(can);
static ExtensionBoard extension_board;
static Relais relais(can);
static Selftest selftest(relais, light);
static Console console;
static ConsoleCommandDevice console_command_device(console, relais, light, selftest);
static Button sw1(can);
static Button sw2(can);
static Button sw3(can);
static Button sw4(can);
static Button ext_sw1(can);
static Button ext_sw2(can);
static Button ext_sw3(can);
static Button ext_sw4(can);

extern "C" void app_main() {
    // Create semaphores and tasks
    shutdown_sem = xSemaphoreCreateBinary();

    pin_config.init();

    PinConfig::switch_config_t onboard_switch = pin_config.get_onboard_switch_config();
    PinConfig::switch_config_t ext_board_switch = pin_config.get_ext_board_switch_config();

    can.init(pin_config.get_can_config(), true);
    device.init();

    extension_board.sensor_board_setup(pin_config.get_ext_board_power());

    i2c.init();
    light.init(pin_config.get_onboard_pwm_config(), pin_config.get_ext_board_config());
    relais.init(pin_config.get_onboard_relais_config(), pin_config.get_ext_board_config());
    ext_thsensor.init(pin_config.get_ext_board_onewire(), pin_config.get_ext_board_config());
    //thsensor.init(pin_config.get_onboard_onewire());
    
    //nightlight.init(pin_config.get_ext_board_config());

    //ambient_light_sensor.init(pin_config.get_ext_board_config());
    
    sw1.init(onboard_switch.sw1, Button::button_id_t::SW1);
    sw2.init(onboard_switch.sw2, Button::button_id_t::SW2);
    sw3.init(onboard_switch.sw3, Button::button_id_t::SW3);
    sw4.init(onboard_switch.sw4, Button::button_id_t::SW4);
    
//if (ext_thsensor.active || ambient_light_sensor.active() || light.ext_active() || relais.ext_active())
//    {
        presence_sensor.init(pin_config.get_ext_board_pir());
//    }
//    else
//    {
        //extension_board.button_board_setup(pin_config.get_ext_board_power());
        //ext_sw1.init(ext_board_switch.sw1, Button::button_id_t::EXT_SW1);
        //ext_sw2.init(ext_board_switch.sw2, Button::button_id_t::EXT_SW2);
        //ext_sw3.init(ext_board_switch.sw3, Button::button_id_t::EXT_SW3);
        //ext_sw4.init(ext_board_switch.sw4, Button::button_id_t::EXT_SW4);
//    }
    
    // init update at last; rollback will be disabled on init
    update.init(static_cast<uint8_t>(ICAN::DEVICE_t::Button));

    // selftest.init();
    // console.init();

    xSemaphoreTake(shutdown_sem, portMAX_DELAY); // Wait for tasks to complete

    can.deinit();

    // Cleanup
    vSemaphoreDelete(shutdown_sem);
}
