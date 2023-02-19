/*
 * CAN Lightswitch module
*/

#include <cstdio>
#include <cstring>
#include <cstdlib>

#include <freertos/FreeRTOS.h>
#include <freertos/task.h>
#include <freertos/semphr.h>
#include <esp_err.h>
#include <esp_log.h>

#include "esp32-ha-lib/Button.hpp"
#include "esp32-ha-lib/CAN.hpp"
#include "esp32-ha-lib/Device.hpp"
#include "esp32-ha-lib/Update.hpp"
#include "esp32-ha-lib/THSensor.hpp"
#include "esp32-ha-lib/EEPROM.hpp"
#include "esp32-ha-lib/PresenceSensor.hpp"
#include "esp32-ha-lib/I2C.hpp"
#include "esp32-ha-lib/AmbientLightSensor.hpp"
#include "ExtensionBoard.hpp"
#include "Light.hpp"
#include "gpio_definition.hpp"
/* --------------------- Definitions and static variables ------------------ */

#define TAG                     "CANLIGHTSWITCH"

static SemaphoreHandle_t shutdown_sem;


/* --------------------------- Tasks and Functions -------------------------- */

extern "C"
void app_main()
{

    CAN can(RX_GPIO_NUM, TX_GPIO_NUM, true);
    Update update(can);
    Device device(can);
    Light light(can);
    I2C i2c;
    THSensor ext_thsensor(can, EXT_SENSOR_ONEWIRE, EXT_SENSOR_SDA, EXT_SENSOR_SCL);
    THSensor thsensor(can, ONEWIRE_GPIO_NUM);
    EEPROM eeprom(EXT_SENSOR_SDA, EXT_SENSOR_SCL);
    AmbientLightSensor ambient_light_sensor(can, EXT_SENSOR_SDA, EXT_SENSOR_SCL);
    PresenceSensor presence_sensor(can, EXT_SENSOR_OUT);
    ExtensionBoard extension_board;
    Button sw1(can, SW1_GPIO_NUM, Button::button_id_t::SW1);
    Button sw2(can, SW2_GPIO_NUM, Button::button_id_t::SW2);
    Button sw3(can, SW3_GPIO_NUM, Button::button_id_t::SW3);
    Button sw4(can, SW4_GPIO_NUM, Button::button_id_t::SW4);
    Button ext_sw1(can, EXT_BUTTON_SW1, Button::button_id_t::EXT_SW1);
    Button ext_sw2(can, EXT_BUTTON_SW2, Button::button_id_t::EXT_SW2);
    Button ext_sw3(can, EXT_BUTTON_SW3, Button::button_id_t::EXT_SW3);
    Button ext_sw4(can, EXT_BUTTON_SW4, Button::button_id_t::EXT_SW4);

    //Create semaphores and tasks
    shutdown_sem  = xSemaphoreCreateBinary();

    can.init();
    device.init();
    
    extension_board.sensor_board_setup();
    
    i2c.init();
    
    ext_thsensor.init();
    thsensor.init();
    /*
    eeprom.init();
    auto eeprom_found = eeprom.probe();
    if (!eeprom_found)
    {
        eeprom.deinit();
    }
    */

    ambient_light_sensor.init();
    
    if (ext_thsensor.active/* && !eeprom_found*/)
    {
        presence_sensor.init();
    }
    
    sw1.init();
    sw2.init();
    sw3.init();
    sw4.init();
    /*
    if (!(ext_thsensor.active || eeprom_found))
    {
        extension_board.button_board_setup();
        ext_sw1.init();
        ext_sw2.init();
        ext_sw3.init();
        ext_sw4.init();
    }
    */
    // init update at last; rollback will be disabled on init
    update.init(static_cast<uint8_t>(ICAN::DEVICE_t::Button));
    
    xSemaphoreTake(shutdown_sem, portMAX_DELAY);    //Wait for tasks to complete

    can.deinit();

    //Cleanup
    vSemaphoreDelete(shutdown_sem);
}
