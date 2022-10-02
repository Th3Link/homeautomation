#include "can2mqtt.hpp"
#include "helper.h"
#include <cstring>
#include <iostream>

template<>
void CAN2MQTT<TemperatureSensor>::subscribe()
{
}

template<>
void CAN2MQTT<TemperatureSensor>::mqtt_data(const char* topic, unsigned int topic_len, 
    const char* data, unsigned int data_len)
{
}

template<>
void CAN2MQTT<TemperatureSensor>::can_data(uint32_t identifier, uint8_t* data, 
    unsigned int data_len)
{
    if (CAN_ID_COMPARE(identifier, can2mqtt_e::TemperatureSensor))
    {
        union {
            uint8_t data[8];
            struct
            {
                uint64_t id : 48;
                uint64_t temperature : 16;
            };
        } t;

        for (auto i = 0; i <=7; i++)
        {
            t.data[i] = data[i];
        };

        //temperature sensors
        std::string temperatureTopic = "canbus/temperature/"
        + toHexString(t.id);
        
        std::string temperatureData = std::to_string(
            static_cast<double>(t.temperature)/16.0);
        mqtt_publish(temperatureTopic.c_str(), temperatureData.c_str());
    }
}
