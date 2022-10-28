#pragma once

#include "esp32-ha-lib/ICAN.hpp"
#include "IMQTT.hpp"

class BridgeLamps : public ICANDispatcher, public IMQTTDispatcher
{
public:
    BridgeLamps(ICAN&, IMQTT&);
    void init();
    bool dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request) override;
    void dispatch(const char* topic, size_t topic_len, const char* data, size_t data_len) override;
    void connected_event() override;
private:
    ICAN& m_can;
    IMQTT& m_mqtt;
    static const char* TAG;
};
