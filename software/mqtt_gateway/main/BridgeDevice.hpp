#pragma once

#include "esp32-ha-lib/ICAN.hpp"
#include "IMQTT.hpp"
#include "DeviceList.hpp"

class BridgeDevice : public ICANDispatcher, public IMQTTDispatcher
{
public:
    BridgeDevice(ICAN&, IMQTT&, DeviceList&);
    void init();
    bool dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request) override;
    void dispatch(const char* topic, size_t topic_len, const char* data, size_t data_len) override;
    void connected_event() override;
private:
    ICAN& m_can;
    IMQTT& m_mqtt;
    DeviceList& m_device_list;
    static const char* TAG;
};
