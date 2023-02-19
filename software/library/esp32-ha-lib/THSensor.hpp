#pragma once

#include "ICAN.hpp"
#include <driver/gpio.h>
#include <bme680.h>

class THSensor
{
public:
    THSensor(ICAN&, gpio_num_t onewire_pin, gpio_num_t sda_pin, gpio_num_t scl_pin);
    THSensor(ICAN&, gpio_num_t onewire_pin);
    void init();
    bool active = false;
    void dispatch(uint16_t value, uint64_t id, ICAN::MSG_ID_t);
    gpio_num_t onewire_pin();
    bme680_t* bme680_sensor();
    static constexpr unsigned int LOOP_DELAY_MS = 200000;
    static constexpr unsigned int MAX_SENSORS = 20;
    static constexpr unsigned int RESCAN_INTERVAL = 8;
    static const char* TAG;

private:
    ICAN& m_can;
    gpio_num_t m_onewire_pin;
    bool m_use_i2s_sensors;
    bme680_t m_bme680;
};
