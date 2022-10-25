#pragma once

#include "esp32-ha-lib/ICAN.hpp"

class Light: public ICANDispatcher
{
public:
    Light(ICAN&);
    void set(uint8_t, uint8_t);
    static const char* TAG;
    bool dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request) override;
private:
    ICAN& m_can;
};
