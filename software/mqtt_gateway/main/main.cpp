#include "esp_err.h"
#include "esp_log.h"
#include "freertos/FreeRTOS.h"
#include "freertos/semphr.h"
#include "freertos/task.h"
#include <driver/gpio.h>
#include <driver/twai.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "BridgeButton.hpp"
#include "BridgeDebug.hpp"
#include "BridgeDevice.hpp"
#include "BridgeLamps.hpp"
#include "BridgeRelais.hpp"
#include "CANUpdate.hpp"
#include "ConfigStorage.hpp"
#include "ConsoleCommandDevice.hpp"
#include "DeviceList.hpp"
#include "Echo.hpp"
#include "LAN.hpp"
#include "Logging.hpp"
#include "MQTT.hpp"
#include "Network.hpp"
#include "Update.hpp"
#include "Web.hpp"
#include "WiFi.hpp"
#include "esp32-ha-lib/CAN.hpp"
#include "esp32-ha-lib/Console.hpp"
#include "esp32-ha-lib/PinConfig.hpp"
#include "nvs_flash.h"

/* --------------------- Definitions and static variables ------------------ */
// Example Configuration
#define DATA_PERIOD_MS 50
#define NO_OF_ITERS 3
#define ITER_DELAY_MS 1000
#define RX_TASK_PRIO 8 // Receiving task priority

static const char *TAG = "main";

static SemaphoreHandle_t shutdown_sem;

static PinConfig pin_config;
static MQTT mqtt;
static Logging can_logging(mqtt);
static Update update;
static CANUpdate can_update(can_logging);
static DeviceList device_list(can_logging);
static Network network;
static WiFi wifi;
static LAN lan;
static Web web(update, can_update, mqtt, can_logging, wifi, network, can_logging, device_list);
static BridgeDevice bridge_device(can_logging, mqtt, device_list);
static BridgeRelais bridge_relais(can_logging, mqtt, device_list);
static BridgeButton bridge_button(can_logging, mqtt, device_list);
static BridgeLamps bridge_lamps(can_logging, mqtt, device_list);
static BridgeDebug bridge_debug(can_logging, mqtt, device_list);
static Echo echo(can_logging);
static Console console;
static ConsoleCommandDevice console_command_device(console);
static ConfigStorage config_storage;

extern "C" void app_main() {
    // Initialize NVS
    esp_err_t ret = nvs_flash_init();
    if (ret == ESP_ERR_NVS_NO_FREE_PAGES || ret == ESP_ERR_NVS_NEW_VERSION_FOUND) {
        ESP_LOGI(TAG, "NVS ERROR: Eraseing...");
        ESP_ERROR_CHECK(nvs_flash_erase());
        ret = nvs_flash_init();
    }
    ESP_ERROR_CHECK(ret);

    config_storage.init();

    // network.init will to lan.init and wifi.init
    network.init(wifi, lan);
    can_update.init();
    web.init();

    can_logging.init(pin_config.get_can_config(), false);
    device_list.init();
    // Create semaphores and tasks
    shutdown_sem = xSemaphoreCreateBinary();

    mqtt.init();
    bridge_device.init();
    bridge_relais.init();
    bridge_button.init();
    bridge_lamps.init();
    bridge_debug.init();
    echo.init();
    update.verified();
    console.init();
    xSemaphoreTake(shutdown_sem, portMAX_DELAY); // Wait for tasks to complete

    twai_stop();
    // Uninstall CAN driver
    ESP_ERROR_CHECK(twai_driver_uninstall());
    ESP_LOGI(TAG, "Driver uninstalled");

    // Cleanup
    vSemaphoreDelete(shutdown_sem);
}
