#include "BME680.hpp"
#include <bme680.h>
#include <esp_err.h>
#include <esp_log.h>
#include <freertos/FreeRTOS.h>
#include <freertos/task.h>
#include <nvs_flash.h>
#include <sys/time.h>
static void bme680_task(void *this_ptr) {
    bme680_t sensor;
    memset(&sensor, 0, sizeof(bme680_t));
    ESP_ERROR_CHECK(
        bme680_init_desc(&sensor, BME680_I2C_ADDR_1, I2C_NUM_1, GPIO_NUM_15, GPIO_NUM_16));
    // init the sensor
    ESP_ERROR_CHECK(bme680_init_sensor(&sensor));

    // Changes the oversampling rates to 4x oversampling for temperature
    // and 2x oversampling for humidity. Pressure measurement is skipped.
    bme680_set_oversampling_rates(&sensor, BME680_OSR_4X, BME680_OSR_NONE, BME680_OSR_2X);

    // Change the IIR filter size for temperature and pressure to 7.
    bme680_set_filter_size(&sensor, BME680_IIR_SIZE_7);

    // Change the heater profile 0 to 200 degree Celsius for 100 ms.
    // bme680_set_heater_profile(&sensor, 0, 200, 100);
    // bme680_use_heater_profile(&sensor, 0);

    // Set ambient temperature to 10 degree Celsius
    bme680_set_ambient_temperature(&sensor, 20);

    // as long as sensor configuration isn't changed, duration is constant
    uint32_t duration;
    bme680_get_measurement_duration(&sensor, &duration);

    TickType_t last_wakeup = xTaskGetTickCount();

    bme680_values_float_t values;
    while (1) {
        // trigger the sensor to start one TPHG measurement cycle
        if (bme680_force_measurement(&sensor) == ESP_OK) {
            // passive waiting until measurement results are available
            vTaskDelay(duration);

            // get the results and do something with them
            if (bme680_get_results_float(&sensor, &values) == ESP_OK)
                printf("BME680 Sensor: %.2f °C, %.2f %%, %.2f hPa, %.2f Ohm\n", values.temperature,
                       values.humidity, values.pressure, values.gas_resistance);
        }
        // passive waiting until 1 second is over
        vTaskDelayUntil(&last_wakeup, pdMS_TO_TICKS(1000));
    }
}

const char *BME680::TAG = "BME680";

BME680::BME680(ICAN &ic) : m_can(ic), m_active(false) {
    memset(&m_bme680, 0, sizeof(i2c_dev_t));
}

void BME680::init(PinConfig::i2c_config_t i2c) {

    // xTaskCreate(bme680_task, "bme680_task", configMINIMAL_STACK_SIZE * 4, NULL, 5, NULL);
}

bool BME680::active() {
    return m_active;
}

int8_t BME680::i2c_write(uint8_t reg_addr, const uint8_t *reg_data_ptr, uint32_t data_len) {
    if (i2c_dev_write_reg(&m_bme680.i2c_dev, reg_addr, reg_data_ptr, data_len) == ESP_OK) {
        return 0;
    }
    ESP_LOGE(BME680::TAG, "i2c_read failed\n");
    return 1;
}

int8_t BME680::i2c_read(uint8_t reg_addr, uint8_t *reg_data_ptr, uint32_t data_len) {
    if (i2c_dev_read_reg(&m_bme680.i2c_dev, reg_addr, reg_data_ptr, data_len) == ESP_OK) {
        return 0;
    }
    ESP_LOGE(BME680::TAG, "i2c_read failed\n");
    return 1;
}

void BME680::dispatch(uint16_t value, uint64_t id, ICAN::MSG_ID_t messageId) {
    struct sensor_data_t {
        uint8_t id[6];
        uint16_t value;
    };

#pragma pack(push, 1)
    union {
        uint64_t id_data64;
        uint8_t id_data8[8];
    };
#pragma pack(pop)

#pragma pack(push, 1)
    union {
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
