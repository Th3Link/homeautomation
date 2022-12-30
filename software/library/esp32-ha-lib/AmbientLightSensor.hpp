#pragma once

#include "ICAN.hpp"
#include <button.h>
#include <veml7700.h>


class AmbientLightSensor
{
public:
    AmbientLightSensor(ICAN&, gpio_num_t sda_pin, gpio_num_t scl_pin);
    void init();
    bool probe();
    void read();
    static const char* TAG;
private:
    ICAN& m_can;
    i2c_dev_t m_device;
    veml7700_config_t m_config;
};
