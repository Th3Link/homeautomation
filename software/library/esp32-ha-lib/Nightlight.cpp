#include <freertos/FreeRTOS.h>
#include <freertos/task.h>
#include <esp_err.h>
#include <esp_log.h>
#include <esp_mac.h>
#include "Nightlight.hpp"

const char* Nightlight::TAG = "Nightlight";

Nightlight::Nightlight(ICAN& ic) : 
    m_can(ic), m_active(false)
{
    m_can.add_dispatcher(this);
    memset(&m_pcf8574, 0, sizeof(m_pcf8574));
}

void Nightlight::init(PinConfig::i2c_config_t i2c)
{
    if (i2c.sda == GPIO_NUM_NC || i2c.scl == GPIO_NUM_NC)
    {
        return;
    }
    
    ESP_ERROR_CHECK(pcf8574_init_desc(&m_pcf8574, 0x38, i2c.port, i2c.sda, i2c.scl));
    m_pcf8574.cfg.sda_pullup_en = true;
    m_pcf8574.cfg.scl_pullup_en = true;

    if (pcf8574_port_write(&m_pcf8574, 0xFF) == ESP_OK)
    {
        m_active = true;
    }
}

bool Nightlight::dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request)
{
    switch (static_cast<ICAN::MSG_ID_t>(identifier & 0xFF))
    {
        case ICAN::MSG_ID_t::NIGHTLIGHT:
        {
            if (!m_active)
            {
                // error return here
                return false;
            }
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
