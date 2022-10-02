#include "can2mqtt.hpp"
#include "helper.h"
#include <string>

template<>
void CAN2MQTT<General>::subscribe()
{
}

template<>
void CAN2MQTT<General>::mqtt_data(const char* topic, unsigned int topic_len, 
    const char* data, unsigned int data_len)
{
}

template<>
void CAN2MQTT<General>::can_data(uint32_t identifier, uint8_t* data, 
    unsigned int data_len)
{
    if (CAN_ID_COMPARE(identifier, can2mqtt_e::General))
    {
        //when receiving
        if (CAN_MSG_COMPARE(identifier, 0))
        {
            std::string topic = "canbus/device_state/0x" + toHexString(identifier);
            std::string data_mqtt = "";
            if (data_len == 1)
            {
                data_mqtt = std::to_string(data[0]);
            }
            mqtt_publish(topic.c_str(), data_mqtt.c_str());
        }
    }
}
