#include "can2mqtt.hpp"
#include "helper.h"
#include <string>

template<>
void CAN2MQTT<ButtonNG>::subscribe()
{
}

template<>
void CAN2MQTT<ButtonNG>::mqtt_data(const char* topic, unsigned int topic_len, 
    const char* data, unsigned int data_len)
{
}

template<>
void CAN2MQTT<ButtonNG>::can_data(uint32_t identifier, uint8_t* data, 
    unsigned int data_len)
{
    if (CAN_ID_COMPARE(identifier, can2mqtt_e::ButtonNG))
    {
        if ((CAN_GET_MSG(identifier) >= 92) && (CAN_GET_MSG(identifier) <= 100))
        {
            std::string buttonStateTopic = "canbus/button_state/0x" + 
                toHexString(CAN_GET_NOT_MSG(identifier)) + 
                "/" + std::to_string(CAN_GET_MSG(identifier) - 32);
            std::string buttonStateData = std::to_string(data[0]);
            mqtt_publish(buttonStateTopic.c_str(), buttonStateData.c_str());
        }
    }
}
