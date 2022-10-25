#pragma once

#include "ICAN.hpp"

class Device : ICANDispatcher
{
public:

    Device(ICAN&);
    void init();
    bool dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, 
        bool request) override;
    static const char* TAG;
private:
    ICAN& m_can;
};
