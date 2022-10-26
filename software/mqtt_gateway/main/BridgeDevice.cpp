#include "BridgeDevice.hpp"
#include "helper.hpp"

const char* BridgeDevice::TAG = "BridgeDevice";
const char canbusavailable_topic[] = "canbus/available/";
BridgeDevice::BridgeDevice(ICAN& ic, IMQTT& im) : m_can(ic), m_mqtt(im)
{
    //m_mqtt.add_dispatcher(this);
    m_can.add_dispatcher(this);
}

void BridgeDevice::init()
{

}

bool BridgeDevice::dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request)
{
    if (ICAN::MSG_COMPARE(identifier, ICAN::MSG_ID_t::TEMPERATURE_SENSOR))
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
        m_mqtt.publish(temperatureTopic.c_str(), temperatureData.c_str());
        return true;
    }
    if (ICAN::MSG_COMPARE(identifier, ICAN::MSG_ID_t::HUMIDITY_SENSOR))
    {
        union {
            uint8_t data[8];
            struct
            {
                uint64_t id : 48;
                uint64_t humidity : 16;
            };
        } t;

        for (auto i = 0; i <=7; i++)
        {
            t.data[i] = data[i];
        };

        //temperature sensors
        std::string humidityTopic = "canbus/humidity/"
        + toHexString(t.id);
        
        std::string humidityData = std::to_string(
            static_cast<double>(t.humidity)/16.0);
        m_mqtt.publish(humidityTopic.c_str(), humidityData.c_str());
        return true;
    }
    if (ICAN::MSG_COMPARE(identifier, ICAN::MSG_ID_t::AVAILABLE))
    {
        std::string availableTopic = std::string(canbusavailable_topic)
        + toHexString(identifier);
    }
    return false;
}

void BridgeDevice::dispatch(const char* topic, size_t topic_len, const char* data, size_t data_len)
{

}

void BridgeDevice::connected()
{
    std::string availableTopic = std::string(canbusavailable_topic)
    + toHexString(ICAN::DID_TO_ID(m_can.get_id()) + ICAN::TYPE_TO_ID(static_cast<ICAN::DEVICE_t>(m_can.get_type())) + 
        ICAN::ID_NG_MASK);
    std::string availableData = "0x01";
    m_mqtt.publish(availableTopic.c_str(), availableData.c_str());
}
