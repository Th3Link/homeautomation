#include "can2mqtt.hpp"
#include "helper.h"
#include <cstring>
#include <string>
#include <cstdio>

const char canbuslamps_topic[] = "canbus/lamp_command/#";

template<>
void CAN2MQTT<Lamps>::subscribe()
{
    mqtt_subscribe(canbuslamps_topic);
}

template<>
void CAN2MQTT<Lamps>::mqtt_data(const char* topic, unsigned int topic_len, 
    const char* data, unsigned int data_len)
{
    if (strncmp(topic,canbuslamps_topic,sizeof(canbuslamps_topic)-2) == 0)
    {
        uint32_t id = hextoInt(mqtt_split(topic,topic_len,2));
        uint32_t lamp_no = std::stoi(mqtt_split(topic,topic_len,3));
        if (lamp_no <= 88)
        {
            id = id + lamp_no + 64;
            
            union {
                uint8_t data8[8];
                uint64_t data64;
            };
            
            data8[0] = std::stoi(std::string(data, data_len)) & 0xFF;
            
            can_send(id, data8, 1, false);
        }
        else
        {
            id = id + 90;
            
            union {
                uint8_t data8[8];
                uint64_t data64;
            };
            
            data8[0] = std::stoi(std::string(data, data_len)) & 0xFF;
            data8[1] = lamp_no & 0xFF;
            data8[2] = (lamp_no >> 8 ) & 0xFF;
            data8[3] = (lamp_no >> 16) & 0xFF;
            can_send(id, data8, 4, false);
        }
    }
}

template<>
void CAN2MQTT<Lamps>::can_data(uint32_t identifier, uint8_t* data, 
    unsigned int data_len)
{
    if (CAN_ID_COMPARE(identifier, can2mqtt_e::Lamps))
    {
        if ((CAN_GET_MSG(identifier) >= 32) && (CAN_GET_MSG(identifier) <= 56))
        {
            std::string lampStateTopic = "canbus/lamp_state/0x" + 
                toHexString(CAN_GET_NOT_MSG(identifier)) + "/" + 
                std::to_string((CAN_GET_MSG(identifier)) - 32);
            std::string lampStateData = std::to_string(data[0]);
            mqtt_publish(lampStateTopic.c_str(), lampStateData.c_str());
        }
        
        if (CAN_GET_MSG(identifier) == 58)
        {
            
            uint32_t lamp_mask = (data[1] & 0xFF) | ((data[2] << 16) & 0xFF) | 
            ((data[3] << 24) & 0xFF);

            std::string lampStateTopic = "canbus/lamp_state/0x" + 
                toHexString(CAN_GET_NOT_MSG(identifier)) + "/" + 
                std::to_string(lamp_mask);
            std::string lampStateData = std::to_string(data[0]);
            mqtt_publish(lampStateTopic.c_str(), lampStateData.c_str());
        }
    }
}
