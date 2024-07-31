#include <freertos/FreeRTOS.h>
#include <esp_ota_ops.h>
#include <nvs_flash.h>
#include <esp_log.h>

#include "Update.hpp"

const char* Update::TAG = "Update";

Update::Update(ICAN& ic) : m_can(ic)
{
    m_can.add_dispatcher(this);
}

void Update::init(const uint8_t const_type)
{
    nvs_handle_t nvs_handle;
    nvs_open("storage", NVS_READWRITE, &nvs_handle);
    uint8_t type = 0;
    if (nvs_get_u8(nvs_handle, "can_type", &type) != ESP_OK)
    {
        nvs_set_u8(nvs_handle, "can_type", const_type);
        nvs_commit(nvs_handle);
        nvs_get_u8(nvs_handle, "can_type", &type);
    }

    nvs_close(nvs_handle);
    esp_ota_mark_app_valid_cancel_rollback();
}

bool Update::dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request)
{
    static bool update_mode = false;
    static esp_ota_handle_t ota_handle;
    static const esp_partition_t* partition = esp_ota_get_next_update_partition(NULL);
    //constexpr size_t BUFFER_SIZE = 256;
    //static uint8_t buffer[2][BUFFER_SIZE];
    //static uint32_t buffer_idx[2] {0};
    switch (static_cast<ICAN::MSG_ID_t>(identifier & 0xFF))
    {
        case ICAN::MSG_ID_t::FLASH_WRITE:
        {
            esp_ota_write(ota_handle, data, data_len);
            return true;
        }
        
        case ICAN::MSG_ID_t::AVAILABLE:
        {
            if (request && !update_mode)
            {
                uint8_t data[1] {static_cast<uint8_t>(ICAN::AVAILABLE_t::APPLICATION)};
                m_can.send(ICAN::MSG_ID_t::AVAILABLE, data, sizeof(data), false);
            }
            if (request && update_mode)
            {
                ESP_LOGI(TAG, "changed to update mode\n");
                uint8_t data[1] {static_cast<uint8_t>(ICAN::AVAILABLE_t::UPDATE_MODE)};
                m_can.send(ICAN::MSG_ID_t::AVAILABLE, data, sizeof(data), false);
            }
            return true;
        }
        
        case ICAN::MSG_ID_t::RESTART:
        {
            if (data[0] == static_cast<uint8_t>(ICAN::AVAILABLE_t::UPDATE_MODE))
            {
                update_mode = true;
                esp_ota_begin(partition, OTA_SIZE_UNKNOWN, &ota_handle);
                ESP_LOGI(TAG, "change to update mode");
            }
            else
            {
                if (update_mode)
                {
                    ESP_LOGI(TAG, "update complete, restarting");
                    esp_ota_end(ota_handle);
                    esp_ota_set_boot_partition(partition);
                    esp_restart();
                }
                else
                {
                    // just do a normal restart as requested
                    esp_restart();
                }
                update_mode = false;
            }
            return true;
        }
        
        case ICAN::MSG_ID_t::FLASH_VERIFY:
        {
            if (request)
            {
                uint8_t checksum[CONFIG_APP_RETRIEVE_LEN_ELF_SHA] {0};
                esp_partition_get_sha256(partition, checksum);
                m_can.send(ICAN::MSG_ID_t::FLASH_VERIFY, checksum, std::min(CONFIG_APP_RETRIEVE_LEN_ELF_SHA, 8), false);
                ESP_LOGI(TAG, "verify checksum 0x%02x%02x%02x%02x%02x%02x%02x%02x",
                    checksum[0], checksum[1], checksum[2], checksum[3],
                    checksum[4], checksum[5], checksum[6], checksum[7]);
            }
            return true;
        }
        default:
            break;
    }
    return false;
}
