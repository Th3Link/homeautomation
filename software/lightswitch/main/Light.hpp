#pragma once

#include "esp32-ha-lib/ICAN.hpp"
#include <pca9685.h>

class Light: public ICANDispatcher
{
public:
    Light(ICAN& ic, gpio_num_t sda_pin, gpio_num_t scl_pin);
    void init();
    void set_onboard(ICAN::LAMP_MSG_t&);
    void set_ext(ICAN::LAMP_MSG_t&);
    static const char* TAG;
    bool dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request) override;
private:
    static constexpr uint8_t PWM_CHANNELS = 16;
    static constexpr uint8_t PWM_EXTENDER = 16;
    
    struct pwm_extender_t {
        bool active;
        i2c_dev_t dev;
        uint16_t value[PWM_CHANNELS];
    };
    
    ICAN& m_can;
    pwm_extender_t m_pca9685[PWM_EXTENDER];
};
