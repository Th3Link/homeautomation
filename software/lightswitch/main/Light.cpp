#include <esp_err.h>
#include <esp_log.h>
#include <driver/gpio.h>
#include <driver/ledc.h>
#include "Light.hpp"
#include "gpio_definition.hpp"
#include <algorithm>

const char* Light::TAG = "Light";

#define LEDC_TIMER              LEDC_TIMER_0
#define LEDC_MODE               LEDC_LOW_SPEED_MODE
#define LEDC_CHANNEL            LEDC_CHANNEL_0
#define LEDC_DUTY_RES           LEDC_TIMER_13_BIT // Set duty resolution to 13 bits
#define LEDC_DUTY               (8191) // Set duty to 100%. ((2 ** 13) - 1) = 8191
#define LEDC_FREQUENCY          (1000) // Frequency in Hertz. Set frequency at 5 kHz

struct gpio_channel_map_t {
    ledc_channel_t channel;
    gpio_num_t gpio;
};

static const gpio_channel_map_t gpio_channel_map[] {
    {LEDC_CHANNEL_1, DIM2_GPIO_NUM},
    {LEDC_CHANNEL_2, DIM6_GPIO_NUM},
    {LEDC_CHANNEL_3, DIM1_GPIO_NUM},
    {LEDC_CHANNEL_4, DIM5_GPIO_NUM},
    {LEDC_CHANNEL_5, DIM4_GPIO_NUM},
    {LEDC_CHANNEL_6, DIM8_GPIO_NUM}
//    {LEDC_CHANNEL_2, DIM3_GPIO_NUM},
//    {LEDC_CHANNEL_6, DIM7_GPIO_NUM},
    
};

Light::Light(ICAN& ic) : m_can(ic)
{
    m_can.add_dispatcher(this);
    
    // Prepare and then apply the LEDC PWM timer configuration
    ledc_timer_config_t ledc_timer = {
        .speed_mode       = LEDC_MODE,
        .duty_resolution  = LEDC_DUTY_RES,
        .timer_num        = LEDC_TIMER,
        .freq_hz          = LEDC_FREQUENCY,  // Set output frequency at 5 kHz
        .clk_cfg          = LEDC_AUTO_CLK
    };
    ESP_ERROR_CHECK(ledc_timer_config(&ledc_timer));
    
    for (auto& gpio_channel_map_entry : gpio_channel_map)
    {
        // Prepare and then apply the LEDC PWM channel configuration
        ledc_channel_config_t ledc_channel = {
            .gpio_num       = gpio_channel_map_entry.gpio,
            .speed_mode     = LEDC_MODE,
            .channel        = gpio_channel_map_entry.channel,
            .intr_type      = LEDC_INTR_DISABLE,
            .timer_sel      = LEDC_TIMER,
            .duty           = 0, // Set duty to 0%
            .hpoint         = 0,
            .flags          = {
                .output_invert = 0
            }
        };
        ESP_ERROR_CHECK(ledc_channel_config(&ledc_channel));
    }
}

void Light::set(uint8_t num, uint8_t duty)
{
    ESP_ERROR_CHECK(ledc_set_duty(LEDC_MODE, static_cast<ledc_channel_t>(num), LEDC_DUTY*duty/255));
    ESP_ERROR_CHECK(ledc_update_duty(LEDC_MODE, static_cast<ledc_channel_t>(num)));
}

bool Light::dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request)
{
    union {
        ICAN::LAMP_MSG_t lamps;
        uint8_t data8[sizeof(ICAN::RELAIS_MSG_t)];
    };
    
    for(auto& d : data8)
    {
        d = 0;
    }
    for (unsigned int i = 0; i < std::min(data_len, sizeof(ICAN::RELAIS_MSG_t)); i++)
    {
        data8[i] = data[i];
    }
    
    switch (static_cast<ICAN::MSG_ID_t>(identifier & 0xFF))
    {
        case ICAN::MSG_ID_t::LAMP_GROUP:
        {
            for (unsigned int i = 0; i < 6; i++)
            {
                if (lamps.bitmask & (1 << i))
                {
                    set(i, lamps.value);
                }
            }
            return true;
        }
        default:
            break;
    }
    return false;
}
