#pragma once

#include "ICAN.hpp"

class Update : ICANDispatcher
{
public:
    Update(ICAN&);
    void init(const uint8_t const_type);
    void dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request);
    static const char* TAG;
private:
    ICAN& m_can;
};
