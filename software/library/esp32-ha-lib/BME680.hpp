#pragma once

#include "PinConfig.hpp"
#include "ICAN.hpp"
#include <bme680.h>

class BME680
{
public:
    BME680(ICAN& ic);
    void init(PinConfig::i2c_config_t);
    bool active();
    static const char* TAG;
    int8_t i2c_write(uint8_t reg_addr, const uint8_t *reg_data_ptr, uint32_t data_len);
    int8_t i2c_read(uint8_t reg_addr, uint8_t *reg_data_ptr, uint32_t data_len);
    void dispatch(uint16_t value, uint64_t id, ICAN::MSG_ID_t);
private:
    ICAN& m_can;
    bool m_active;
    bme680_t m_bme680;
};
