#include "BridgeLamps.hpp"
#include "helper.hpp"

const char canbuslamps_topic[] = "canbus/lamp_command/#";

const char* BridgeLamps::TAG = "BridgeLamps";

BridgeLamps::BridgeLamps(ICAN& ic, IMQTT& im) : m_can(ic), m_mqtt(im)
{
    //m_can.add_dispatcher(this);
    m_mqtt.add_dispatcher(this);

}

void BridgeLamps::init()
{

}

void BridgeLamps::dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request)
{
    //noting to do here; there is no feed back
}

void BridgeLamps::dispatch(const char* topic, size_t topic_len, const char* data, size_t data_len)
{
    bool lampscommand = (strncmp(topic,canbuslamps_topic,sizeof(canbuslamps_topic)-2) == 0);
    if (lampscommand)
    {
        uint32_t id = hextoInt(mqtt_split(topic,topic_len,2));
        union {
            uint8_t data8[4];
            uint32_t value_bitmask;
        };
        value_bitmask = std::stoi(std::string(data, data_len));
        m_can.send(id + static_cast<uint32_t>(ICAN::MSG_ID_t::LAMP_GROUP), data8, 4, false);
    }
}

void BridgeLamps::connected()
{
    m_mqtt.subscribe(canbuslamps_topic);
}
