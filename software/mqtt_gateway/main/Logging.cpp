#include "Logging.hpp"
#include "helper.hpp"

#include <string>

const char* Logging::TAG = "Logging";

Logging::Logging(PinConfig::can_config_t can_config, bool enable_filter, IMQTT& im) : 
    CAN(can_config, enable_filter), m_mqtt(im), m_mqtt_logging(false)
{
    
}

void Logging::init()
{
    CAN::init();
}

void Logging::mqtt_log(uint32_t id, uint8_t* data, unsigned int data_len, bool request)
{
    if (!m_mqtt_logging)
    {
        return;
    }
    std::string topic("canbus/log/0x");
    topic += toHexString(id);
    std::string mqtt_data;
    mqtt_data += (request ? "R" : "0x");
    for (unsigned int i = 0; i<data_len; i++)
    {
        mqtt_data += toHexStringPad(data[i]);
    }
    m_mqtt.publish(topic.c_str(), mqtt_data.c_str());
}

void Logging::send(uint32_t id, uint8_t* data, unsigned int data_len, bool request)
{
    mqtt_log(id, data, data_len, request);
    CAN::send(id, data, data_len, request);
}
bool Logging::dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request)
{
    mqtt_log(identifier, data, data_len, request);
    return CAN::dispatch(identifier, data, data_len, request); 
}

void Logging::mqtt_logging(bool enable)
{
    m_mqtt_logging = enable;
}

bool Logging::mqtt_logging()
{
    return m_mqtt_logging;
}
