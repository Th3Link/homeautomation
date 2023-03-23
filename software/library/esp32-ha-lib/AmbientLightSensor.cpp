// std
#include <cstdint>

// lib
#include <freertos/FreeRTOS.h>
#include <freertos/task.h>
#include <esp_err.h>
#include <esp_log.h>
#include <driver/i2c.h>
#include <veml7700.h>
// local
#include "AmbientLightSensor.hpp"

const char* AmbientLightSensor::TAG = "AmbientLightSensor";

void ambient_light_sensor_task(void *this_ptr)
{
    auto ambient_light_sensor = reinterpret_cast<AmbientLightSensor*>(this_ptr);
    
    while (1)
    {
        vTaskDelay(pdMS_TO_TICKS(5000));
        ambient_light_sensor->read();
    }
}

AmbientLightSensor::AmbientLightSensor(ICAN& ic) : m_can(ic),
    m_active(false)
{
    memset(&m_device, 0, sizeof(i2c_dev_t));
}

void AmbientLightSensor::init(PinConfig::i2c_config_t i2c)
{
    if (i2c.sda == GPIO_NUM_NC || i2c.scl == GPIO_NUM_NC)
    {
        return;
    }
    ESP_ERROR_CHECK(veml7700_init_desc(&m_device, i2c.port, i2c.sda, i2c.scl));

    m_config.gain = VEML7700_GAIN_1;
    m_config.integration_time = VEML7700_INTEGRATION_TIME_400MS;
    m_config.persistence_protect = VEML7700_PERSISTENCE_PROTECTION_4;
    m_config.interrupt_enable = 0;
    m_config.shutdown = 0;
    m_config.power_saving_enable = 1;
    m_config.power_saving_mode = VEML7700_POWER_SAVING_MODE_4000MS;
    
    if (probe())
    {
        veml7700_set_config(&m_device, &m_config);
        ESP_LOGI(TAG, "start_task");
        m_active = true;
        xTaskCreate(ambient_light_sensor_task, "als_task",  configMINIMAL_STACK_SIZE * 4, 
            this, 5, NULL);
    }
    else
    {
        m_active = false;
    }
}

bool AmbientLightSensor::probe()
{
    m_device.addr = VEML7700_I2C_ADDR;
    esp_err_t e = veml7700_probe(&m_device);
    ESP_LOGI(TAG, "veml7700_probe error code: %x", e);
    return (e == ESP_OK);
}

void AmbientLightSensor::read()
{
    union {
        uint32_t als;
        uint8_t als_8[4];
    };
    als = 0;
    
    m_device.addr = VEML7700_I2C_ADDR;
    
    veml7700_get_ambient_light(&m_device, &m_config, &als);   
    m_can.send(ICAN::MSG_ID_t::AMBIENT_LIGHT_SENSOR, als_8, 4, false);
}

bool AmbientLightSensor::active()
{
    return m_active;
}
