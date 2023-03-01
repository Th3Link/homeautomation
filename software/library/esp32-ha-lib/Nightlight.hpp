#pragma once

#include "ICAN.hpp"
#include <driver/gpio.h>
#include <pcf8574.h>

class Nightlight : public ICANDispatcher
{
public:
    Nightlight(ICAN&, gpio_num_t sda_pin, gpio_num_t scl_pin);
    void init();
    bool dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request) override;
    bool active();
    static const char* TAG;

private:
    ICAN& m_can;
    i2c_dev_t m_pcf8574;
    bool m_active;
};
