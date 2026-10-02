#pragma once

#include <driver/gpio.h>

class EEPROM
{
public:
    EEPROM(gpio_num_t sda_pin, gpio_num_t scl_pin);
    void init();
    void deinit();
    bool probe();
    static const char* TAG;
private:
    gpio_num_t m_sda_pin;
    gpio_num_t m_scl_pin;
};
