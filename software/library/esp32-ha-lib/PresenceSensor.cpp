// std
#include <cstdint>

// lib
#include <freertos/FreeRTOS.h>
#include <freertos/task.h>
#include <esp_err.h>
#include <esp_log.h>

// local
#include "PresenceSensor.hpp"

const char* PresenceSensor::TAG = "PresenceSensor";

static bool pir_state_released = true;
static bool next_pir_state_released = true;

void pir_dispatch(button_t *b, button_state_t s)
{
    if (s == BUTTON_PRESSED)
    {
        pir_state_released = false;
        next_pir_state_released = false;
    }
    else if (s == BUTTON_RELEASED)
    {
        pir_state_released = true;
    }
}

void pir_smooth_task(void *this_ptr)
{
    #define EXT_PIR 8
    auto presence_sensor = reinterpret_cast<PresenceSensor*>(this_ptr);
    struct button_data_t {
        uint8_t identifier;
        ICAN::BUTTON_EVENT_t state;
        uint16_t count;
    };
    #pragma pack(push,1)
    union
    {
        button_data_t button_data;
        uint8_t data[4];
    };
    #pragma pack(pop)
    button_data.identifier = EXT_PIR;
    button_data.state = ICAN::BUTTON_EVENT_t::RELEASED;
    while (1)
    {
        next_pir_state_released = pir_state_released;
        vTaskDelay(pdMS_TO_TICKS(1000));
        if (next_pir_state_released && pir_state_released)
        {
            if (button_data.state == ICAN::BUTTON_EVENT_t::RELEASED)
            {
                continue;
            }
            button_data.state = ICAN::BUTTON_EVENT_t::RELEASED;
            button_data.count = 0;
        }
        else
        {
            if (button_data.count == 0)
            {
                button_data.state = ICAN::BUTTON_EVENT_t::PRESSED;
            }
            else
            {
                button_data.state = ICAN::BUTTON_EVENT_t::HOLD;
            }
            button_data.count++;
        }
        //ESP_LOGI(presence_sensor->TAG, "Dispatch PIR state %d\n", 
        //    static_cast<uint8_t>(button_data.state));
        presence_sensor->send(data, sizeof(data));
    }
}

PresenceSensor::PresenceSensor(ICAN& ic) : m_can(ic)
{
}

void PresenceSensor::init(gpio_num_t out_pin)
{
    if (out_pin == GPIO_NUM_NC)
    {
        return;
    }

    m_out_pir.gpio = out_pin;
    m_out_pir.pressed_level = 1;
    m_out_pir.internal_pull = true;
    m_out_pir.autorepeat = true;
    m_out_pir.callback = pir_dispatch;
    m_out_pir.ctx = this;
    
    ESP_ERROR_CHECK(button_init(&m_out_pir));
    xTaskCreate(pir_smooth_task, "pir_smooth_task",  configMINIMAL_STACK_SIZE * 4, 
        this, 5, NULL);
}

void PresenceSensor::deinit()
{
    button_done(&m_out_pir);
}

void PresenceSensor::send(uint8_t* data, unsigned int data_len)
{
    m_can.send(ICAN::MSG_ID_t::PIR_SENSOR, data, data_len, false);
}
