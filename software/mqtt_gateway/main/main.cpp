#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "freertos/semphr.h"
#include "esp_err.h"
#include "esp_log.h"
#include <driver/gpio.h>
#include <driver/twai.h>

#include "nvs_flash.h"
#include "Update.hpp"
#include "CANUpdate.hpp"
#include "Logging.hpp"
#include "MQTT.hpp"
#include "WiFi.hpp"
#include "DeviceList.hpp"
#include "Web.hpp"
#include "esp32-ha-lib/CAN.hpp"
#include "BridgeDevice.hpp"
#include "BridgeLamps.hpp"
#include "BridgeRelais.hpp"
#include "BridgeButton.hpp"
#include "BridgeDebug.hpp"

/* --------------------- Definitions and static variables ------------------ */
//Example Configuration
#define DATA_PERIOD_MS                  50
#define NO_OF_ITERS                     3
#define ITER_DELAY_MS                   1000
#define RX_TASK_PRIO                    8       //Receiving task priority
constexpr gpio_num_t TX_GPIO_NUM        = GPIO_NUM_13;
constexpr gpio_num_t RX_GPIO_NUM        = GPIO_NUM_14;

static const char *TAG = "main";

static SemaphoreHandle_t shutdown_sem;

//static CAN can(RX_GPIO_NUM, TX_GPIO_NUM, false);
static MQTT mqtt;
static Logging can_logging(RX_GPIO_NUM, TX_GPIO_NUM, false, mqtt);
static Update update;
static CANUpdate can_update(can_logging);
static DeviceList device_list(can_logging);
static WiFi wifi;
static Web web(update, can_update, mqtt, can_logging, wifi, can_logging, device_list);
static BridgeDevice bridge_device(can_logging, mqtt);
static BridgeRelais bridge_relais(can_logging, mqtt);
static BridgeButton bridge_button(can_logging, mqtt);
static BridgeLamps bridge_lamps(can_logging, mqtt);
static BridgeDebug bridge_debug(can_logging, mqtt);

extern "C"
void app_main()
{   
    //Initialize NVS
    esp_err_t ret = nvs_flash_init();
    if (ret == ESP_ERR_NVS_NO_FREE_PAGES || ret == ESP_ERR_NVS_NEW_VERSION_FOUND) {
      ESP_ERROR_CHECK(nvs_flash_erase());
      ret = nvs_flash_init();
    }
    ESP_ERROR_CHECK(ret);
    wifi.init();
    web.init();
    
    can_logging.init();
    device_list.init();
    //Create semaphores and tasks
    shutdown_sem  = xSemaphoreCreateBinary();

    if (wifi.mode() == WiFi::Mode::Client)
    {
        mqtt.init();
        bridge_device.init();
        bridge_relais.init();
        bridge_button.init();
        bridge_lamps.init();
        bridge_debug.init();
    }
    
    update.verified();
    
    xSemaphoreTake(shutdown_sem, portMAX_DELAY);    //Wait for tasks to complete

    twai_stop();
    //Uninstall CAN driver
    ESP_ERROR_CHECK(twai_driver_uninstall());
    ESP_LOGI(TAG, "Driver uninstalled");

    //Cleanup
    vSemaphoreDelete(shutdown_sem);
}
