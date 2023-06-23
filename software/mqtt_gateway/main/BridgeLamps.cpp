#include "BridgeLamps.hpp"
#include "helper.hpp"
#include <string>

const char canbuslamps_topic[] = "canbus/lamp_command/#";
const char canbusnightlight_topic[] = "canbus/nightlight_command/#";

const char* BridgeLamps::TAG = "BridgeLamps";

BridgeLamps::BridgeLamps(ICAN& ic, IMQTT& im, DeviceList& dl) : m_can(ic), m_mqtt(im), m_device_list(dl)
{
    //m_can.add_dispatcher(this);
    m_mqtt.add_dispatcher(this);

}

void BridgeLamps::init()
{

}

bool BridgeLamps::dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request)
{
    //noting to do here; there is no feed back
    return false;
}

void BridgeLamps::dispatch(const char* topic, size_t topic_len, const char* data, size_t data_len)
{
    bool lampscommand = (strncmp(topic,canbuslamps_topic,sizeof(canbuslamps_topic)-2) == 0);
    if (lampscommand)
    {
        uint32_t id = m_device_list.resolve(mqtt_split(topic,topic_len,2));
        union {
            uint8_t data8[sizeof(ICAN::LAMP_MSG_t)];
            ICAN::LAMP_MSG_t  lamps;
        };
        lamps.bank = 0;
        lamps.bitmask = std::stoi(mqtt_split(data, data_len,1), nullptr, 16);
        lamps.value = static_cast<uint8_t>(std::stoi(mqtt_split(data, data_len,0), nullptr, 10));
        std::string bank = mqtt_split(data, data_len,2);
        if (bank != "")
        {
            lamps.bank = static_cast<uint8_t>(std::stoi(bank, nullptr, 10));
            m_can.send(id + static_cast<uint32_t>(ICAN::MSG_ID_t::LAMP_GROUP), data8, 
                sizeof(ICAN::LAMP_MSG_t), false);
        }
        else
        {
            m_can.send(id + static_cast<uint32_t>(ICAN::MSG_ID_t::LAMP_GROUP), data8, 
                4, false);
        }
    }
    
    bool nightlightcommand = (strncmp(topic,canbusnightlight_topic,
        sizeof(canbusnightlight_topic)-2) == 0);
    if (nightlightcommand)
    {
        uint32_t id = m_device_list.resolve(mqtt_split(topic,topic_len,2));
        std::string str(data, data_len);
        uint8_t val = std::stoi(str);
        
        m_can.send(id + static_cast<uint32_t>(ICAN::MSG_ID_t::NIGHTLIGHT), &val, 1, false);
    }
}

void BridgeLamps::connected_event()
{
    m_mqtt.subscribe(canbuslamps_topic);
    m_mqtt.subscribe(canbusnightlight_topic);
}
