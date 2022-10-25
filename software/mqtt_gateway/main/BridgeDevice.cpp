#include "BridgeDevice.hpp"
#include "helper.hpp"

const char* BridgeDevice::TAG = "BridgeDevice";

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
    return false;
}

void BridgeDevice::dispatch(const char* topic, size_t topic_len, const char* data, size_t data_len)
{

}

void BridgeDevice::connected()
{
    
}
