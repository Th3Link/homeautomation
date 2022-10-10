#pragma once

#include "ICAN.hpp"
#include <button.h>
#include <driver/gpio.h>

class PresenceSensor
{
public:
    PresenceSensor(ICAN&, gpio_num_t out_pin);
    void init();
    void deinit();
    void send(uint8_t* data, unsigned int data_len);
    static const char* TAG;
private:
    ICAN& m_can;
    button_t m_out_pir;
};
