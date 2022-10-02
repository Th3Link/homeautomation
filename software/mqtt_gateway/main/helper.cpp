// std
#include <cstdint>
#include <string>
#include <algorithm>
#include <sstream>

// lib

// local
#include "helper.h"
#include "can2mqtt.hpp"

std::string toHexString(unsigned int n)
{
    std::stringstream ss;
    ss << std::hex << n;
    return ss.str();
}

unsigned int hextoInt(std::string s)
{
    return std::stoul(s, nullptr, 16);
}

void requestSubscribe()
{
    CAN2MQTT<RelaisNG>::subscribe();
    CAN2MQTT<Lamps>::subscribe();
    CAN2MQTT<ButtonNG>::subscribe();
    CAN2MQTT<TemperatureSensor>::subscribe();
}

void mqtt_dispatch(const char* topic, unsigned int topic_len, 
    const char* data, unsigned int data_len)
{
    CAN2MQTT<RelaisNG>::mqtt_data(topic, topic_len, data, data_len);
    CAN2MQTT<Lamps>::mqtt_data(topic, topic_len, data, data_len);
    CAN2MQTT<ButtonNG>::mqtt_data(topic, topic_len, data, data_len);
    CAN2MQTT<TemperatureSensor>::mqtt_data(topic, topic_len, data, data_len);
}

void can_dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len)
{
    CAN2MQTT<RelaisNG>::can_data(identifier, data, data_len);
    CAN2MQTT<Lamps>::can_data(identifier, data, data_len);
    CAN2MQTT<ButtonNG>::can_data(identifier, data, data_len);
    CAN2MQTT<TemperatureSensor>::can_data(identifier, data, data_len);
}

std::string mqtt_split(const char* in, unsigned int len, unsigned int pos)
{
    std::string str(in, len);
    char delim = '/';
    
	size_t start;
	size_t end = 0;
    unsigned int i = 0;
	std::string split;
    while ((start = str.find_first_not_of(delim, end)) != std::string::npos)
	{
		end = str.find(delim, start);
		split = str.substr(start, end - start);
        if (i == pos)
        {
            return split;
        }
        i++;
	}
    split = "";
    return split;
    
}
