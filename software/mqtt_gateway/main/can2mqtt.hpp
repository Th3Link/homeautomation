#ifndef __CAN2MQTT_HPP__
#define __CAN2MQTT_HPP__

#include <string>

enum can2mqtt_e
{
    General            = 0x00,
    RelaisNG           = 0x02,
    Lamps              = 0x03,
    TemperatureSensor  = 0x01,
    ButtonNG           = 0x04,
};

std::string mqtt_split(const char* in, unsigned int len, unsigned int pos);
std::string toHexString(unsigned int);

constexpr uint32_t CAN_ID_NG_MASK       = 0x10000000; // 1 bit
constexpr uint32_t CAN_ID_GROUP_MASK    = 0x0FC00000; // 6 bits
constexpr uint32_t CAN_ID_TYPE_MASK     = 0x003F0000; // 6 bits
constexpr uint32_t CAN_ID_ID_MASK       = 0x0000FF00; // 8 bits
constexpr uint32_t CAN_ID_MSG_MASK      = 0x000000FF; // 8 bits

constexpr uint32_t CAN_GET_GROUP(uint32_t id)
{
    return (id & CAN_ID_GROUP_MASK) >> 22;
}

constexpr uint32_t CAN_GET_TYPE(uint32_t id)
{
    return (id & CAN_ID_TYPE_MASK) >> 16;
}

constexpr uint32_t CAN_GET_ID(uint32_t id)
{
    return (id & CAN_ID_ID_MASK) >> 8;
}

constexpr uint32_t CAN_GET_MSG(uint32_t id)
{
    return id & CAN_ID_MSG_MASK;
}

constexpr uint32_t CAN_GET_NOT_MSG(uint32_t id)
{
    return id & ~CAN_ID_MSG_MASK;
}

constexpr bool CAN_ID_COMPARE(uint32_t id, can2mqtt_e type)
{
    return (((id & CAN_ID_NG_MASK) == CAN_ID_NG_MASK) && (CAN_GET_TYPE(id) == type));
}

constexpr bool CAN_MSG_COMPARE(uint32_t id, uint8_t msg_id)
{
    return (((id & CAN_ID_NG_MASK) == CAN_ID_NG_MASK) && (CAN_GET_MSG(id) == msg_id));
}

unsigned int hextoInt(std::string);

template<can2mqtt_e E>
class CAN2MQTT
{
public:
    static void subscribe();
    static void mqtt_data(const char* topic, unsigned int topic_len, 
        const char* data, unsigned int data_len);
    static void can_data(uint32_t identifier, uint8_t* data, unsigned int data_len);
};

#endif //__CAN2MQTT_HPP__
