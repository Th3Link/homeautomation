#pragma once

#include "esp32-ha-lib/ICAN.hpp"
#include "IMQTT.hpp"

class BridgeDebug : public IMQTTDispatcher
{
public:
    BridgeDebug(ICAN&, IMQTT&);
    void init();
    void dispatch(const char* topic, size_t topic_len, const char* data, size_t data_len) override;
    void connected() override;
private:
    ICAN& m_can;
    IMQTT& m_mqtt;
    static const char* TAG;
};
