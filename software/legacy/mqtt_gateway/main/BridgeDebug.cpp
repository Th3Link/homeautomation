#include "BridgeDebug.hpp"
#include "helper.hpp"
#include <esp_system.h>

const char* BridgeDebug::TAG = "BridgeDebug";

const char debug_topic[] = "canbus/debug/#";
const char gateway_restart_topic[] = "canbus/gateway/restart";

BridgeDebug::BridgeDebug(ICAN& ic, IMQTT& im, DeviceList& dl) : m_can(ic), m_mqtt(im), m_device_list(dl)
{
    m_mqtt.add_dispatcher(this);
}

void BridgeDebug::init()
{

}

void BridgeDebug::dispatch(const char* topic, size_t topic_len, const char* data, size_t data_len)
{
    bool debugcommand = (strncmp(topic,debug_topic,sizeof(debug_topic)-2) == 0);
    bool restartcommand = (strncmp(topic,gateway_restart_topic,sizeof(gateway_restart_topic)) == 0);
    if (debugcommand)
    {
        uint32_t id = hextoInt(mqtt_split(topic,topic_len,2));
        uint8_t can_data[8] {0};
        size_t can_data_len {0};
        for (unsigned i = 2; i < data_len; i+=2)
        {
            std::string s(&data[i], 2);
            can_data[(i/2)-1] = static_cast<uint8_t>(std::stoul(s,nullptr, 16));
            can_data_len++;
        }
        std::stoi(mqtt_split(data,data_len,0));
        m_can.send(id, &can_data[0], can_data_len, false);
    }
    if (restartcommand)
    {
        esp_restart();
    }
}

void BridgeDebug::connected_event()
{
    m_mqtt.subscribe(debug_topic);
}
