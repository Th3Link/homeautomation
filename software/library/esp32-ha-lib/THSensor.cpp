#include <dht.h>
#include <ds18x20.h>


#include <freertos/FreeRTOS.h>
#include <freertos/task.h>
#include <esp_err.h>
#include <esp_log.h>
#include <esp_mac.h>
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
    esp_efuse_mac_get_default(chipid);
    while (1)
    {
        if (dht_read_float_data(DHT_TYPE_AM2301, thsensor->onewire_pin(), &humidity, &temperature) == ESP_OK)
        {
            ESP_LOGI(THSensor::TAG, "Humidity: %.1f%% Temp: %.1fC\n", humidity, temperature);
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

static void bme680_task(void *this_ptr)
{
    auto thsensor = reinterpret_cast<THSensor*>(this_ptr);

    // init the sensor
    ESP_ERROR_CHECK(bme680_init_sensor(thsensor->bme680_sensor()));

    // Changes the oversampling rates to 4x oversampling for temperature
    // and 2x oversampling for humidity. Pressure measurement is skipped.
    bme680_set_oversampling_rates(thsensor->bme680_sensor(), BME680_OSR_4X, BME680_OSR_4X, BME680_OSR_4X);

    // Change the IIR filter size for temperature and pressure to 7.
    bme680_set_filter_size(thsensor->bme680_sensor(), BME680_IIR_SIZE_7);

    // Change the heater profile 0 to 200 degree Celsius for 100 ms.
    bme680_use_heater_profile(thsensor->bme680_sensor(), BME680_HEATER_NOT_USED);
    // Set ambient temperature to 10 degree Celsius
    bme680_set_ambient_temperature(thsensor->bme680_sensor(), 21);

    // as long as sensor configuration isn't changed, duration is constant
    uint32_t duration;
    bme680_get_measurement_duration(thsensor->bme680_sensor(), &duration);

    TickType_t last_wakeup = xTaskGetTickCount();

    bme680_values_float_t values;
    while (1)
    {
        // trigger the sensor to start one TPHG measurement cycle
        if (bme680_force_measurement(thsensor->bme680_sensor()) == ESP_OK)
        {
            // passive waiting until measurement results are available
            vTaskDelay(duration);

            // get the results and do something with them
            if (bme680_get_results_float(thsensor->bme680_sensor(), &values) == ESP_OK)
            {
                printf("BME680 Sensor: %.2f °C, %.2f %%, %.2f hPa, %.2f Ohm\n",
                        values.temperature, values.humidity, values.pressure, values.gas_resistance);
            
                thsensor->dispatch(
                    static_cast<uint16_t>(values.temperature*16), 0xBADBAD00, 
                    ICAN::MSG_ID_t::TEMPERATURE_SENSOR);
                thsensor->dispatch(
                    static_cast<uint16_t>(values.humidity*16), 0xBADBAD00, 
                    ICAN::MSG_ID_t::HUMIDITY_SENSOR);
            }
        }
        else
        {
            // get the results and do something with them
            if (bme680_get_results_float(thsensor->bme680_sensor(), &values) == ESP_OK)
            {
                printf("BME680 Sensor: %.2f °C, %.2f %%, %.2f hPa, %.2f Ohm\n",
                        values.temperature, values.humidity, values.pressure, values.gas_resistance);
                thsensor->dispatch(
                    static_cast<uint16_t>(values.temperature*16), 0xBADBAD00, 
                    ICAN::MSG_ID_t::TEMPERATURE_SENSOR);
                thsensor->dispatch(
                    static_cast<uint16_t>(values.humidity*16), 0xBADBAD00, 
                    ICAN::MSG_ID_t::HUMIDITY_SENSOR);
            }
        }
        // passive waiting until 1 second is over
        vTaskDelayUntil(&last_wakeup, pdMS_TO_TICKS(10000));
    }
}

THSensor::THSensor(ICAN& ic, gpio_num_t onewire_pin, gpio_num_t sda_pin, gpio_num_t scl_pin) : 
    m_can(ic), m_onewire_pin(onewire_pin), m_use_i2s_sensors(true)
{
    memset(&m_bme680, 0, sizeof(bme680_t));
    ESP_ERROR_CHECK(bme680_init_desc(&m_bme680, BME680_I2C_ADDR_1, I2C_NUM_1, sda_pin, scl_pin));
    m_bme680.i2c_dev.cfg.sda_pullup_en = true;
    m_bme680.i2c_dev.cfg.scl_pullup_en = true;
}

THSensor::THSensor(ICAN& ic, gpio_num_t onewire_pin) : 
    m_can(ic), m_onewire_pin(onewire_pin), m_use_i2s_sensors(false)
{

}

void THSensor::init()
{
    vTaskDelay(pdMS_TO_TICKS(1000));
    
    // probing BME680
    if (m_use_i2s_sensors)
    {
        if (i2c_dev_probe(&(m_bme680.i2c_dev), I2C_DEV_WRITE) == ESP_OK)
        {
            ESP_LOGI(THSensor::TAG, "Sensor BME680 ok\n");
            xTaskCreate(bme680_task, "bme680_task", configMINIMAL_STACK_SIZE * 4, this, 5, NULL);
            active = true;
        }
        else
        {
            ESP_LOGI(THSensor::TAG, "Sensor BME680 probe failed\n");
        }
    }
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

gpio_num_t THSensor::onewire_pin()
{
    return m_onewire_pin;
}

bme680_t* THSensor::bme680_sensor()
{
    return &m_bme680;
}
