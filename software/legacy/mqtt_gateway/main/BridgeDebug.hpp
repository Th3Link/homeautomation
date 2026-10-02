#pragma once

#include "esp32-ha-lib/ICAN.hpp"
#include "IMQTT.hpp"
#include "DeviceList.hpp"

class BridgeDebug : public IMQTTDispatcher
{
public:
    BridgeDebug(ICAN&, IMQTT&, DeviceList&);
    void init();
    void dispatch(const char* topic, size_t topic_len, const char* data, size_t data_len) override;
    void connected_event() override;
private:
    ICAN& m_can;
    IMQTT& m_mqtt;
    DeviceList& m_device_list;
    static const char* TAG;
};
