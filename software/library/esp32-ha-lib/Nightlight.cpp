#include <freertos/FreeRTOS.h>
#include <freertos/task.h>
#include <esp_err.h>
#include <esp_log.h>
#include <esp_mac.h>
#include "Nightlight.hpp"

const char* Nightlight::TAG = "Nightlight";

Nightlight::Nightlight(ICAN& ic, gpio_num_t sda_pin, gpio_num_t scl_pin) : 
    m_can(ic), m_active(false)
{
    m_can.add_dispatcher(this);
    memset(&m_pcf8574, 0, sizeof(m_pcf8574));
    ESP_ERROR_CHECK(pcf8574_init_desc(&m_pcf8574, 0x38, I2C_NUM_1, sda_pin, scl_pin));
    m_pcf8574.cfg.sda_pullup_en = true;
    m_pcf8574.cfg.scl_pullup_en = true;
}

void Nightlight::init()
{
    if (pcf8574_port_write(&m_pcf8574, 0xFF) == ESP_OK)
    {
        pcf8574_port_write(&m_pcf8574, 0xFF);
        m_active = true;
    }
}

bool Nightlight::dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request)
{
    switch (static_cast<ICAN::MSG_ID_t>(identifier & 0xFF))
    {
        case ICAN::MSG_ID_t::NIGHTLIGHT:
        {
            if (data_len == 1)
            {
                uint8_t val = 0xFF;
                switch (data[0])
                {
                    case 1: val = 0xFE; break;
                    case 2: val = 0xFC; break;
                    case 3: val = 0xF8; break;
                }
                pcf8574_port_write(&m_pcf8574, val);
            }
            return true;
        }
        default:
            break;
    }
    return false;
}

bool Nightlight::active()
{
    return m_active;
}
