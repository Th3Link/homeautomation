#pragma once

#include "ICAN.hpp"
#include <driver/gpio.h>

class THSensor
{
public:
    THSensor(ICAN&, gpio_num_t);
    void init();
    bool active = false;
    void dispatch(uint16_t value, uint64_t id, ICAN::MSG_ID_t);
    gpio_num_t onewire_pin();
    static constexpr unsigned int LOOP_DELAY_MS = 10000;
    static constexpr unsigned int MAX_SENSORS = 20;
    static constexpr unsigned int RESCAN_INTERVAL = 8;
    static const char* TAG;
private:
    ICAN& m_can;
    gpio_num_t m_onewire_pin;
};
