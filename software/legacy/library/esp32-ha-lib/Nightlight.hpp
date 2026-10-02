#pragma once

#include "ICAN.hpp"
#include "PinConfig.hpp"
#include <driver/gpio.h>
#include <pcf8574.h>

class Nightlight : public ICANDispatcher
{
public:
    Nightlight(ICAN&);
    void init(PinConfig::i2c_config_t);
    bool dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request) override;
    bool active();
    static const char* TAG;

private:
    ICAN& m_can;
    i2c_dev_t m_pcf8574;
    bool m_active;
};
