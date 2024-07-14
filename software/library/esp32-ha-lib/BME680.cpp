#include "BME680.hpp"
#include <freertos/FreeRTOS.h>
#include <freertos/task.h>
#include <esp_err.h>
#include <esp_log.h>
#include <nvs_flash.h>
#include <bsec_integration.h>
#include <sys/time.h>

// obtained from the config dir of the BSEC2 release. This is bme680_iaq_33v_3s_28d.
const uint8_t bsec_config_iaq[1974] = 
     {0,0,4,2,189,1,0,0,0,0,0,0,158,7,0,0,176,0,1,0,0,168,19,73,64,49,119,76,0,192,40,72,0,192,40,72,137,65,0,191,205,204,204,190,0,0,64,191,225,122,148,190,10,0,3,0,0,0,96,64,23,183,209,56,0,0,0,0,0,0,0,0,0,0,0,0,205,204,204,189,0,0,0,0,0,0,0,0,0,0,128,63,0,0,0,0,0,0,128,63,0,0,0,0,0,0,0,0,0,0,128,63,0,0,0,0,0,0,128,63,0,0,0,0,0,0,0,0,0,0,128,63,0,0,0,0,0,0,128,63,82,73,157,188,95,41,203,61,118,224,108,63,155,230,125,63,191,14,124,63,0,0,160,65,0,0,32,66,0,0,160,65,0,0,32,66,0,0,32,66,0,0,160,65,0,0,32,66,0,0,160,65,8,0,2,0,236,81,133,66,16,0,3,0,10,215,163,60,10,215,35,59,10,215,35,59,13,0,5,0,0,0,0,0,100,254,131,137,87,88,0,9,0,229,208,34,62,0,0,0,0,0,0,0,0,218,27,156,62,225,11,67,64,0,0,160,64,0,0,0,0,0,0,0,0,94,75,72,189,93,254,159,64,66,62,160,191,0,0,0,0,0,0,0,0,33,31,180,190,138,176,97,64,65,241,99,190,0,0,0,0,0,0,0,0,167,121,71,61,165,189,41,192,184,30,189,64,12,0,10,0,0,0,0,0,0,0,0,0,13,5,11,0,1,1,2,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,10,10,4,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,128,63,0,0,0,88,1,254,0,2,1,5,48,117,100,0,44,1,112,23,151,7,132,3,197,0,92,4,144,1,64,1,64,1,144,1,48,117,48,117,48,117,48,117,100,0,100,0,100,0,48,117,48,117,48,117,100,0,100,0,48,117,48,117,8,7,8,7,8,7,8,7,8,7,100,0,100,0,100,0,100,0,48,117,48,117,48,117,100,0,100,0,100,0,48,117,48,117,100,0,100,0,255,255,255,255,255,255,255,255,255,255,44,1,44,1,44,1,44,1,44,1,44,1,44,1,44,1,44,1,44,1,44,1,44,1,44,1,44,1,255,255,255,255,255,255,255,255,255,255,112,23,112,23,112,23,112,23,8,7,8,7,8,7,8,7,112,23,112,23,112,23,112,23,112,23,112,23,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,112,23,112,23,112,23,112,23,255,255,255,255,220,5,220,5,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,220,5,220,5,220,5,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,44,1,0,5,10,5,0,2,0,10,0,30,0,5,0,5,0,5,0,5,0,5,0,5,0,64,1,100,0,100,0,100,0,200,0,200,0,200,0,64,1,64,1,64,1,10,0,0,0,0,0,213,52,0,0};

static int8_t i2c_write(uint8_t reg_addr, const uint8_t *reg_data_ptr, uint32_t data_len, void *intf_ptr)
{
    auto bme680 = reinterpret_cast<BME680*>(intf_ptr);
    return bme680->i2c_write(reg_addr, reg_data_ptr, data_len);
}

static int8_t i2c_read(uint8_t reg_addr, uint8_t *reg_data_ptr, uint32_t data_len, void *intf_ptr)
{
    auto bme680 = reinterpret_cast<BME680*>(intf_ptr);
    return bme680->i2c_read(reg_addr, reg_data_ptr, data_len);
}

static void delay_us(uint32_t period, void *intf_ptr)
{
    uint32_t delay_ms = period/1000;
    if (delay_ms == 0)
    {
        delay_ms = 1;
    }
    // all delays are quite long (like 1 ms to 1000 ms) so no busy weit here, use the
    // normal tast delay here
    vTaskDelay(pdMS_TO_TICKS(period/1000));
    return;
}

static int64_t get_timestamp_us()
{
    struct timeval tv_now;
    gettimeofday(&tv_now, NULL);
    int64_t system_current_time = (int64_t)tv_now.tv_sec * 1000000L + (int64_t)tv_now.tv_usec;
    return system_current_time;
}

static uint32_t state_load(void* context, uint8_t *state_buffer, uint32_t n_buffer)
{
    size_t length = static_cast<size_t>(n_buffer);
    nvs_handle_t nvs_handle;
    nvs_open("storage", NVS_READONLY, &nvs_handle);
    if (nvs_get_blob(nvs_handle, "bmestate", state_buffer, &length) != ESP_OK)
    {
        length = 0;
    }
    nvs_close(nvs_handle);
    return static_cast<uint32_t>(length);
}

static void state_save(void* context, const uint8_t *state_buffer, uint32_t length)
{
    nvs_handle_t nvs_handle;
    nvs_open("storage", NVS_READWRITE, &nvs_handle);
    nvs_set_blob(nvs_handle, "bmestate", state_buffer, length);
    nvs_commit(nvs_handle);
}

static void output_ready(void* context, measured_values mv, bsec_library_return_t bsec_status)
{
    constexpr int SKIP_COUNT = 10;
    static int send = 0;

    if (--send > 0)
    {
        return;
    }
    send = SKIP_COUNT;
        
    auto bme680 = reinterpret_cast<BME680*>(context);
    union
    {
        uint8_t chipid[8];
        uint64_t chipid64;
    };
    chipid64 = 0;
    chipid[0] = mv.iaq_accuracy;
    /*
    ESP_LOGI(BME680::TAG, "raw_temperature %f", mv.raw_temperature);
    ESP_LOGI(BME680::TAG, "raw_humidity %f", mv.raw_humidity);
    ESP_LOGI(BME680::TAG, "raw_pressure %f", mv.raw_pressure);
    ESP_LOGI(BME680::TAG, "raw_gas %f", mv.raw_gas);
    ESP_LOGI(BME680::TAG, "compensated_temperature %f", mv.compensated_temperature);
    ESP_LOGI(BME680::TAG, "compensated_humidity %f", mv.compensated_humidity);
    ESP_LOGI(BME680::TAG, "iaq %f", mv.iaq);
    ESP_LOGI(BME680::TAG, "co2_ppm %f", mv.co2_ppm);
    ESP_LOGI(BME680::TAG, "voc_ppm %f", mv.voc_ppm);
    ESP_LOGI(BME680::TAG, "iaq_accuracy %d", mv.iaq_accuracy);
    */
    bme680->dispatch(static_cast<uint16_t>(mv.compensated_temperature*16), chipid64,
        ICAN::MSG_ID_t::TEMPERATURE_SENSOR);
    bme680->dispatch(static_cast<uint16_t>(mv.compensated_humidity*16), chipid64,
        ICAN::MSG_ID_t::HUMIDITY_SENSOR);
    bme680->dispatch(static_cast<uint16_t>(mv.raw_pressure*16/100), chipid64,
        ICAN::MSG_ID_t::PRESSURE_SENSOR);
    if (mv.iaq_accuracy >= 1)
    {
        bme680->dispatch(static_cast<uint16_t>(mv.iaq*16), chipid64,
            ICAN::MSG_ID_t::AIR_QUALITY);
        bme680->dispatch(static_cast<uint16_t>(mv.co2_ppm*16), chipid64,
            ICAN::MSG_ID_t::CO2_EQUIVALENT);
        bme680->dispatch(static_cast<uint16_t>(mv.voc_ppm*16), chipid64,
            ICAN::MSG_ID_t::VOC_BREATH);
    }
}

static void bme680_task(void *this_ptr)
{
    auto bme680 = reinterpret_cast<BME680*>(this_ptr);
    
    return_values_init ret;
    struct bme68x_dev bme_dev;
	memset(&bme_dev,0,sizeof(bme_dev)); 
    
    bme_dev.intf_ptr = bme680;
    bme_dev.read = i2c_read;
    bme_dev.write = i2c_write;
    bme_dev.delay_us = delay_us;
    bme_dev.intf = BME68X_I2C_INTF;
    ret = bsec_iot_init(bme680, BSEC_SAMPLE_RATE_LP, state_load, bme_dev);
    if (ret.bme68x_status)
    {
        return;
        //return (int)ret.bme68x_status;
    }
    else if (ret.bsec_status)
    {
        return;
        //return (int)ret.bsec_status;
    }
    
    /* Call to endless loop function which reads and processes data based on sensor settings */
    /* State is saved every 10.000 samples, which means every 10.000 * 3 secs = 500 minutes  */
    bsec_iot_loop(bme680, delay_us, get_timestamp_us, output_ready, state_save, 10000);
    
    while(1)
    {
        vTaskDelay(pdMS_TO_TICKS(1000));
    }
}

const char* BME680::TAG = "BME680";

BME680::BME680(ICAN& ic) : m_can(ic), m_active(false)
{
    memset(&m_bme680, 0, sizeof(i2c_dev_t));
}

void BME680::init(PinConfig::i2c_config_t i2c)
{
    if (i2c.sda == GPIO_NUM_NC || i2c.scl == GPIO_NUM_NC)
    {
        return;
    }
        
    ESP_ERROR_CHECK(bme680_init_desc(&m_bme680, BME680_I2C_ADDR_1, i2c.port, i2c.sda, i2c.scl));
    m_bme680.i2c_dev.cfg.sda_pullup_en = true;
    m_bme680.i2c_dev.cfg.scl_pullup_en = true;
    
    // probing BME680
    if (i2c_dev_probe(&(m_bme680.i2c_dev), I2C_DEV_WRITE) == ESP_OK)
    {
        ESP_LOGI(BME680::TAG, "ok\n");
        xTaskCreate(bme680_task, "bme680_task", configMINIMAL_STACK_SIZE * 4 + 
            BSEC_MAX_WORKBUFFER_SIZE, this, 5, NULL);
        m_active = true;
    }
    else
    {
        ESP_LOGI(BME680::TAG, "probe failed\n");
    }
}

bool BME680::active()
{
    return m_active;
}

int8_t BME680::i2c_write(uint8_t reg_addr, const uint8_t *reg_data_ptr, uint32_t data_len)
{
    if (i2c_dev_write_reg(&m_bme680.i2c_dev, reg_addr, reg_data_ptr, data_len) == ESP_OK)
    {
        return 0;
    }
    ESP_LOGE(BME680::TAG, "i2c_read failed\n");
    return 1;
}

int8_t BME680::i2c_read(uint8_t reg_addr, uint8_t *reg_data_ptr, uint32_t data_len)
{
    if (i2c_dev_read_reg(&m_bme680.i2c_dev, reg_addr, reg_data_ptr, data_len) == ESP_OK)
    {
        return 0;
    }
    ESP_LOGE(BME680::TAG, "i2c_read failed\n");
    return 1;
}

void BME680::dispatch(uint16_t value, uint64_t id, ICAN::MSG_ID_t messageId)
{
    struct sensor_data_t {
        uint8_t id[6];
        uint16_t value;
    };
    
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
