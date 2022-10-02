#include "can2mqtt.hpp"
#include "helper.h"
#include <cstring>
#include <string>

const char canbusrelais_topic[] = "canbus/relais_command/#";
const char canbusrollershutter_topic[] = "canbus/rollershutter_command/#";

template<>
void CAN2MQTT<RelaisNG>::subscribe()
{
    mqtt_subscribe(canbusrelais_topic);
    mqtt_subscribe(canbusrollershutter_topic);
}

template<>
void CAN2MQTT<RelaisNG>::mqtt_data(const char* topic, unsigned int topic_len, 
    const char* data, unsigned int data_len)
{
    if (strncmp(topic,canbusrelais_topic,sizeof(canbusrelais_topic)-2) == 0)
    {
        uint32_t id = hextoInt(mqtt_split(topic,topic_len,2));
        uint32_t relais_no = std::stoi(mqtt_split(topic,topic_len,3));
        id = id + relais_no + 64;
        
        union {
            uint8_t data_bytes[8];
            uint64_t data;
        } du;
        
        du.data = std::stoi(std::string(data, data_len));
        
        can_send(id, du.data_bytes, 8, false);
    }
    if (strncmp(topic,canbusrollershutter_topic,sizeof(canbusrollershutter_topic)-2) == 0)
    {
        uint32_t id = hextoInt(mqtt_split(topic,topic_len,2));
        uint32_t relais_no = std::stoi(mqtt_split(topic,topic_len,3));
        id = id + relais_no + 128;
        
        union {
            uint8_t data_bytes[8];
            struct {
                uint64_t state : 8;
                uint64_t stop_time : 24;
                uint64_t reserved : 32;
            };
        } du;
        
        du.state = std::stoi(mqtt_split(data,data_len,0));
        du.stop_time = std::stoi(mqtt_split(data,data_len,1));
        du.reserved = 0;
        can_send(id, du.data_bytes, 8, false);
    }
}

template<>
void CAN2MQTT<RelaisNG>::can_data(uint32_t identifier, uint8_t* data, 
    unsigned int data_len)
{
    if (CAN_ID_COMPARE(identifier, can2mqtt_e::RelaisNG))
    {
        //when receiving
        if ((CAN_GET_MSG(identifier) >= 32) && (CAN_GET_MSG(identifier) <= 43))
        {
            std::string relaisStateTopic = "canbus/relais_state/0x" + 
                toHexString(CAN_GET_NOT_MSG(identifier)) + "/" + 
                std::to_string(CAN_GET_MSG(identifier) - 32);
            std::string relaisStateData = std::to_string(data[0]);
            mqtt_publish(relaisStateTopic.c_str(), relaisStateData.c_str());
        }
        else if ((CAN_GET_MSG(identifier) >= 96) && (CAN_GET_MSG(identifier) <= 101))
        {
            std::string relaisStateTopic = "canbus/rollershutter_state/0x" + 
                toHexString(CAN_GET_NOT_MSG(identifier)) + "/" + 
                std::to_string(CAN_GET_MSG(identifier) - 96);
            std::string relaisStateData = std::to_string(data[0]);
            mqtt_publish(relaisStateTopic.c_str(), relaisStateData.c_str());
        }
    }
}
