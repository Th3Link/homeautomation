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

#include "lib/Button.hpp"
#include "lib/CAN.hpp"
#include "lib/Device.hpp"
#include "lib/Update.hpp"
#include "lib/THSensor.hpp"
#include "lib/EEPROM.hpp"
#include "lib/PresenceSensor.hpp"
#include "Relais.hpp"
#include "ExtensionBoard.hpp"
#include "gpio_definition.hpp"
/* --------------------- Definitions and static variables ------------------ */

#define TAG                     "CANRELAIS"

static SemaphoreHandle_t shutdown_sem;


/* --------------------------- Tasks and Functions -------------------------- */

extern "C"
void app_main()
{
    CAN can(RX_GPIO_NUM, TX_GPIO_NUM);
    Device device(can);
    Update update(can);
    THSensor ext_thsensor(can, EXT_SENSOR_ONEWIRE);
    THSensor thsensor(can, ONEWIRE_GPIO_NUM);
    EEPROM eeprom(EXT_SENSOR_SDA, EXT_SENSOR_SCL);
    PresenceSensor presence_sensor(can, EXT_SENSOR_OUT);
    ExtensionBoard extension_board;
    Relais relais(SDA_GPIO_NUM, SCL_GPIO_NUM, can);
    Button ext_sw1(can, EXT_BUTTON_SW1, Button::button_id_t::EXT_SW1);
    Button ext_sw2(can, EXT_BUTTON_SW2, Button::button_id_t::EXT_SW2);
    Button ext_sw3(can, EXT_BUTTON_SW3, Button::button_id_t::EXT_SW3);
    Button ext_sw4(can, EXT_BUTTON_SW4, Button::button_id_t::EXT_SW4);

    //Create semaphores and tasks
    shutdown_sem  = xSemaphoreCreateBinary();

    can.init();
    device.init();
    
    extension_board.sensor_board_setup();
    
    ext_thsensor.init();
    thsensor.init();

    relais.init();
    
    eeprom.init();
    auto eeprom_found = eeprom.probe();
    if (!eeprom_found)
    {
        eeprom.deinit();
    }
    
    if (ext_thsensor.active && !eeprom_found)
    {
        presence_sensor.init();
    }
    
    if (!(ext_thsensor.active || eeprom_found))
    {
        extension_board.button_board_setup();
        ext_sw1.init();
        ext_sw2.init();
        ext_sw3.init();
        ext_sw4.init();
    }
    
    // init update at last; rollback will be disabled on init
    update.init();
    
    xSemaphoreTake(shutdown_sem, portMAX_DELAY);    //Wait for tasks to complete

    can.deinit();

    //Cleanup
    vSemaphoreDelete(shutdown_sem);
}
