#include <dht.h>
#include <ds18x20.h>

#include <freertos/FreeRTOS.h>
#include <freertos/task.h>
#include <esp_err.h>
#include <esp_log.h>
#include "THSensor.hpp"

struct sensor_data_t {
    uint8_t id[6];
    uint16_t value;
};

const char* THSensor::TAG = "THSensor";

void THSensor::dispatch(uint16_t value, uint64_t id, ICAN::MSG_ID_t messageId)
{
    #pragma pack(push,1)
    union
    {
        uint64_t id_data64;
        uint8_t id_data8[8];
    };
    #pragma pack(pop)
    
    #pragma pack(push,1)
    union
    {
        sensor_data_t sensor_data;
        uint8_t data[8];
    };
    #pragma pack(pop)
    
    id_data64 = id;
    
    sensor_data.id[0] = id_data8[1];
    sensor_data.id[1] = id_data8[2];
    sensor_data.id[2] = id_data8[3];
    sensor_data.id[3] = id_data8[4];
    sensor_data.id[4] = id_data8[5];
    sensor_data.id[5] = id_data8[6];
    sensor_data.value = value;
    
    m_can.send(messageId, data, sizeof(data), false);
}

static void dht_task(void *this_ptr)
{
    float temperature, humidity;
    auto thsensor = reinterpret_cast<THSensor*>(this_ptr);
    union
    {
        uint8_t chipid[8];
        uint64_t chipid64;
    };
    chipid64 = 0;
    chipid[0] = 32;
    while (1)
    {
        if (dht_read_float_data(DHT_TYPE_AM2301, thsensor->onewire_pin(), &humidity, &temperature) == ESP_OK)
        {
            thsensor->dispatch(
                static_cast<uint16_t>(temperature*16), chipid64 + thsensor->onewire_pin(), 
                ICAN::MSG_ID_t::TEMPERATURE_SENSOR);
            thsensor->dispatch(
                static_cast<uint16_t>(humidity*16), chipid64 + thsensor->onewire_pin(), 
                ICAN::MSG_ID_t::HUMIDITY_SENSOR);
        }
        else
        {
            ESP_LOGE(THSensor::TAG, "Could not read data from sensor\n");
        }
        // If you read the sensor data too often, it will heat up
        // http://www.kandrsmith.org/RJS/Misc/Hygrometers/dht_sht_how_fast.html
        vTaskDelay(pdMS_TO_TICKS(THSensor::LOOP_DELAY_MS));
    }
}

static void ds18x20_task(void *this_ptr)
{
    auto thsensor = reinterpret_cast<THSensor*>(this_ptr);
    ds18x20_addr_t addrs[THSensor::MAX_SENSORS];
    float temps[THSensor::MAX_SENSORS];
    size_t sensor_count = 0;

    esp_err_t res;
    while (1)
    {
        // Every RESCAN_INTERVAL samples, check to see if the sensors connected
        // to our bus have changed.
        res = ds18x20_scan_devices(thsensor->onewire_pin(), addrs, THSensor::MAX_SENSORS, &sensor_count);
        if (res != ESP_OK)
        {
            ESP_LOGE(THSensor::TAG, "Sensors scan error %d (%s)", res, esp_err_to_name(res));
            continue;
        }

        if (!sensor_count)
        {
            ESP_LOGW(THSensor::TAG, "No sensors detected!");
            continue;
        }

        ESP_LOGI(THSensor::TAG, "%d sensors detected", sensor_count);

        // If there were more sensors found than we have space to handle,
        // just report the first MAX_SENSORS..
        if (sensor_count > THSensor::MAX_SENSORS)
            sensor_count = THSensor::MAX_SENSORS;

        // Do a number of temperature samples, and print the results.
        for (int i = 0; i < THSensor::RESCAN_INTERVAL; i++)
        {
            ESP_LOGI(THSensor::TAG, "Measuring...");

            res = ds18x20_measure_and_read_multi(thsensor->onewire_pin(), addrs, sensor_count, temps);
            if (res != ESP_OK)
            {
                ESP_LOGE(THSensor::TAG, "Sensors read error %d (%s)", res, esp_err_to_name(res));
                continue;
            }

            for (int j = 0; j < sensor_count; j++)
            {
                float temp_c = temps[j];
                float temp_f = (temp_c * 1.8) + 32;
                // Float is used in printf(). You need non-default configuration in
                // sdkconfig for ESP8266, which is enabled by default for this
                // example. See sdkconfig.defaults.esp8266
                ESP_LOGI(THSensor::TAG, "Sensor %08" PRIx32 "%08" PRIx32 " (%s) reports %.3f°C (%.3f°F)",
                        (uint32_t)(addrs[j] >> 32), (uint32_t)addrs[j],
                        (addrs[j] & 0xff) == DS18B20_FAMILY_ID ? "DS18B20" : "DS18S20",
                        temp_c, temp_f);
                thsensor->dispatch(static_cast<uint16_t>(temp_c*16), 
                    addrs[j], ICAN::MSG_ID_t::TEMPERATURE_SENSOR);
            }

            // Wait for a little bit between each sample (note that the
            // ds18x20_measure_and_read_multi operation already takes at
            // least 750ms to run, so this is on top of that delay).
            vTaskDelay(pdMS_TO_TICKS(THSensor::LOOP_DELAY_MS));
        }
    }
}

THSensor::THSensor(ICAN& ic) : 
    m_can(ic), m_onewire_pin(GPIO_NUM_NC), m_bme680(ic)
{
    
}

void THSensor::init(gpio_num_t p_onewire_pin)
{
    if (p_onewire_pin == GPIO_NUM_NC)
    {
        return;
    }
    
    m_onewire_pin = p_onewire_pin;
    
    ds18x20_addr_t addrs[MAX_SENSORS];
    size_t sensor_count = 0;
    for (int i = 0; i < 2; i++)
    {
        if (ds18x20_scan_devices(onewire_pin(), addrs, MAX_SENSORS, &sensor_count) == ESP_OK)
        {
            if (sensor_count > 0)
            {
                ESP_LOGI(THSensor::TAG, "Sensor DS18x20 ok\n");
                xTaskCreate(ds18x20_task, "ds18x20_task", configMINIMAL_STACK_SIZE * 4, this, 5, NULL);
                active = true;
                return;
            }
        }
        vTaskDelay(pdMS_TO_TICKS(1000));
    }
    // probing DHT22
    for (int i = 0; i < 2; i++)
    {
        float temperature, humidity;
        if (dht_read_float_data(DHT_TYPE_AM2301, onewire_pin(), &humidity, &temperature) == ESP_OK)
        {
            ESP_LOGI(THSensor::TAG, "Sensor DHT22 ok\n");
            active = true;
            xTaskCreate(dht_task, "dht_task", configMINIMAL_STACK_SIZE * 3, this, 5, NULL);
            return;
        }
        else
        {
            vTaskDelay(pdMS_TO_TICKS(1000));
        }
    }
}

void THSensor::init(gpio_num_t onewire_pin, PinConfig::i2c_config_t i2c)
{
    init(onewire_pin);
    m_bme680.init(i2c);
    if (m_bme680.active())
    {
        active = true;
    }
}

gpio_num_t THSensor::onewire_pin()
{
    return m_onewire_pin;
}
