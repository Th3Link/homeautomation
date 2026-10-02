#pragma once

#include "esp32-ha-lib/ICAN.hpp"
#include "IMQTT.hpp"
#include "DeviceList.hpp"
#include <string>

class BridgeDevice : public ICANDispatcher, public IMQTTDispatcher
{
public:
    struct sensor_data_t
    {
        double value;
        std::string name;
        std::string id;
        std::string additional_id;
        std::string custom_string;
    } result;

    BridgeDevice(ICAN&, IMQTT&, DeviceList&);
    void init();
    bool dispatch_sensor(sensor_data_t sensor_data);
    static std::string sensor_name(ICAN::MSG_ID_t msg_id);
    sensor_data_t sensor_value(uint32_t identifier, uint8_t* data, unsigned int data_len);
    bool dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len,
        bool request) override;
    void dispatch(const char* topic, size_t topic_len, const char* data,
        size_t data_len) override;
    void connected_event() override;
private:
    ICAN& m_can;
    IMQTT& m_mqtt;
    DeviceList& m_device_list;
    static const char* TAG;
};
