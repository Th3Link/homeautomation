#pragma once

#include "ICAN.hpp"
#include "PinConfig.hpp"
#include <button.h>
#include <veml7700.h>


class AmbientLightSensor
{
public:
    AmbientLightSensor(ICAN&);
    void init(PinConfig::i2c_config_t);
    bool probe();
    void read();
    bool active();
    static const char* TAG;
private:
    ICAN& m_can;
    i2c_dev_t m_device;
    veml7700_config_t m_config;
    bool m_active;
};
