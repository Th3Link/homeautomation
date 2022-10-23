#pragma once

#include <cstdint>
#include "IMQTT.hpp"

class Logging
{
public:
    enum class OUTPUT_t : uint8_t
    {
        MQTT,
        WEB
    };
    Logging(IMQTT&);
    void can_log(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request);
    void options();
private:
    static const char* TAG;
    IMQTT& m_mqtt;
};
