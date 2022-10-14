#include "update.h"

#include <freertos/FreeRTOS.h>
#include <esp_ota_ops.h>
#include <nvs_flash.h>
#include <esp_log.h>

static const char *TAG = "update";

static bool update_mode = false;
static esp_ota_handle_t ota_handle;
static const esp_partition_t* partition = NULL;

void update_start()
{
    update_mode = true;
    partition = esp_ota_get_next_update_partition(NULL);
    esp_ota_begin(partition, OTA_WITH_SEQUENTIAL_WRITES, &ota_handle);
    ESP_LOGI(TAG, "change to update mode\n");
}

void update_abort()
{
    esp_ota_abort(ota_handle);
}

bool update_data(char* data, uint32_t data_len)
{
    return (esp_ota_write(ota_handle, data, data_len) == ESP_OK);
}

void update_complete()
{
    ESP_LOGI(TAG, "update complete, restarting\n");
    esp_ota_end(ota_handle);
    esp_ota_set_boot_partition(partition);
    esp_restart();
}

void update_verified()
{
    esp_ota_mark_app_valid_cancel_rollback();
}
