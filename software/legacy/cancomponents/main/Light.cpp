#include <esp_err.h>
#include <esp_log.h>
#include <driver/gpio.h>
#include <driver/ledc.h>
#include "Light.hpp"
#include <algorithm>

const char* Light::TAG = "Light";

#define LEDC_TIMER              LEDC_TIMER_0
#define LEDC_MODE               LEDC_LOW_SPEED_MODE
#define LEDC_CHANNEL            LEDC_CHANNEL_0
#define LEDC_DUTY_RES           LEDC_TIMER_13_BIT // Set duty resolution to 13 bits
#define LEDC_DUTY               (8191) // Set duty to 100%. ((2 ** 13) - 1) = 8191
#define LEDC_FREQUENCY          (1000) // Frequency in Hertz. Set frequency at 5 kHz

static const ledc_channel_t ledc_channels[] {
    LEDC_CHANNEL_0,
    LEDC_CHANNEL_1,
    LEDC_CHANNEL_2,
    LEDC_CHANNEL_3,
    LEDC_CHANNEL_4,
    LEDC_CHANNEL_5,
    LEDC_CHANNEL_6,
    LEDC_CHANNEL_7,
};

Light::Light(ICAN& ic) : 
    m_can(ic), m_ext_active(false), m_ext_disabled(true)
{   
    m_can.add_dispatcher(this);
}

void Light::init(PinConfig::pwm_config_t pwm, PinConfig::i2c_config_t i2c)
{
    // Prepare and then apply the LEDC PWM timer configuration
    ledc_timer_config_t ledc_timer = {
        .speed_mode       = LEDC_MODE,
        .duty_resolution  = LEDC_DUTY_RES,
        .timer_num        = LEDC_TIMER,
        .freq_hz          = LEDC_FREQUENCY,  // Set output frequency at 5 kHz
        .clk_cfg          = LEDC_AUTO_CLK
    };
    ESP_ERROR_CHECK(ledc_timer_config(&ledc_timer));
    unsigned int j = 0;
    for (unsigned int i = 0; i < 8; i++)
    {
        if (pwm.dim[i] == GPIO_NUM_NC)
        {
            continue;
        }
        // Prepare and then apply the LEDC PWM channel configuration
        ledc_channel_config_t ledc_channel = {
            .gpio_num       = pwm.dim[i],
            .speed_mode     = LEDC_MODE,
            .channel        = ledc_channels[j],
            .intr_type      = LEDC_INTR_DISABLE,
            .timer_sel      = LEDC_TIMER,
            .duty           = 0, // Set duty to 0%
            .hpoint         = 0,
            .flags          = {
                .output_invert = 0
            }
        };
        ESP_ERROR_CHECK(ledc_channel_config(&ledc_channel));
        j++;
    }
    
    memset(&m_pca9685, 0, sizeof(pwm_extender_t)*PWM_EXTENDER);
    if (i2c.sda == GPIO_NUM_NC || i2c.scl == GPIO_NUM_NC)
    {
        return;
    }
    for (int i = 0; i < PWM_EXTENDER; i++)
    {
        ESP_ERROR_CHECK(pca9685_init_desc(&m_pca9685[i].dev, PCA9685_ADDR_BASE+i, i2c.port, i2c.sda, i2c.scl));
        m_pca9685[i].dev.cfg.sda_pullup_en = true;
        m_pca9685[i].dev.cfg.scl_pullup_en = true;
    }
    
    for (int i = 0; i < PWM_EXTENDER; i++)
    {
        i2c_dev_t* dev = &m_pca9685[i].dev;
        if (i2c_dev_probe(dev, I2C_DEV_WRITE) == ESP_OK)
        {
            m_pca9685[i].active = true;
            ESP_ERROR_CHECK(pca9685_init(dev));
            ESP_ERROR_CHECK(pca9685_restart(dev));
            ESP_ERROR_CHECK(pca9685_set_output_open_drain(dev, true));
            ESP_ERROR_CHECK(pca9685_set_pwm_frequency(dev, LEDC_FREQUENCY));
            ESP_LOGI(Light::TAG, "Found PWM Expander %d\n",i);
            m_ext_active = true;
        }
    }
}
void Light::set_onboard(ICAN::LAMP_MSG_t& lamps)
{
    if (lamps.bank != 0)
    {
        return;
    }
    
    for (unsigned int num = 0; num < 8; num++)
    {
        if (lamps.bitmask & (1 << num))
        {
            ESP_ERROR_CHECK(ledc_set_duty(LEDC_MODE, static_cast<ledc_channel_t>(num), LEDC_DUTY*lamps.value/255));
            ESP_ERROR_CHECK(ledc_update_duty(LEDC_MODE, static_cast<ledc_channel_t>(num)));
        }
    }
}

void Light::set_ext(ICAN::LAMP_MSG_t& lamps)
{
    if (lamps.bank == 0)
    {
        return;
    }
    //decrease to match arrays
    uint8_t num = lamps.bank - 1;
    
    if (num >= PWM_EXTENDER || !m_pca9685[num].active)
    {
        return;
    }
    for (unsigned int i = 0; i < PWM_CHANNELS; i++)
    {
        if (!(lamps.bitmask & (1 << i)))
        {
            continue;
        }
        m_pca9685[num].value[i] = (4095 * lamps.value) / 255;
    }
    ESP_ERROR_CHECK(pca9685_set_pwm_values(&m_pca9685[num].dev, 0, PWM_CHANNELS, m_pca9685[num].value));
}

bool Light::dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request)
{
    union {
        ICAN::LAMP_MSG_t lamps;
        uint8_t data8[sizeof(ICAN::LAMP_MSG_t)];
    };
    
    for(auto& d : data8)
    {
        d = 0;
    }
    for (unsigned int i = 0; i < std::min(data_len, sizeof(ICAN::LAMP_MSG_t)); i++)
    {
        data8[i] = data[i];
    }
    
    switch (static_cast<ICAN::MSG_ID_t>(identifier & 0xFF))
    {
        case ICAN::MSG_ID_t::LAMP_GROUP:
        {
            set_onboard(lamps);
            set_ext(lamps);
            return true;
        }
        default:
            break;
    }
    return false;
}

bool Light::ext_active()
{
    return m_ext_active;
}
