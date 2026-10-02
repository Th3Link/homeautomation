#include "EEPROM.hpp"
#include "eeprom.h"

#include <esp_err.h>
#include <esp_log.h>

const char* EEPROM::TAG = "EEPROM";

EEPROM::EEPROM(gpio_num_t sda_pin, gpio_num_t scl_pin) : m_sda_pin(sda_pin), m_scl_pin(scl_pin)
{
    
}

void EEPROM::init()
{
    init_i2c_master(m_sda_pin, m_scl_pin);
}

void EEPROM::deinit()
{
    deinit_i2c_master();
}

bool EEPROM::probe()
{
    const uint8_t eeprom_address = 0x50;
    const uint16_t starting_address = 0x0000;

    uint8_t data[4];
    
    // EEPROM read test
    esp_err_t rett = eeprom_read(eeprom_address, starting_address, data, sizeof(data));
    ESP_LOGW(TAG, "EEPROM err %d\n", rett);

    //}
    if (rett == ESP_OK)
    {
        ESP_LOGI(TAG, "EEPROM ok\n");
        ESP_LOGI(TAG, "Read byte 0x%02X at address 0x%04X\n", data[0], starting_address);
        return true;
    }
    return false;
}
