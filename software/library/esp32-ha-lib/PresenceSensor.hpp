#pragma once

#include "ICAN.hpp"
#include <button.h>
#include <driver/gpio.h>

class PresenceSensor
{
public:
    PresenceSensor(ICAN&);
    void init(gpio_num_t out_pin);
    void deinit();
    void send(uint8_t* data, unsigned int data_len);
    static const char* TAG;
private:
    ICAN& m_can;
    button_t m_out_pir;
};
