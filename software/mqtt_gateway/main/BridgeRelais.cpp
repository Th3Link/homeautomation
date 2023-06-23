#include "BridgeRelais.hpp"
#include "helper.hpp"

const char canbusrelais_topic[] = "canbus/relais_command/#";
const char canbusrollershutter_topic[] = "canbus/rollershutter_command/#";

const char* BridgeRelais::TAG = "BridgeRelais";

BridgeRelais::BridgeRelais(ICAN& ic, IMQTT& im, DeviceList& dl) : m_can(ic), m_mqtt(im), m_device_list(dl)
{
    m_can.add_dispatcher(this);
    m_mqtt.add_dispatcher(this);
}

void BridgeRelais::init()
{

}

bool BridgeRelais::dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request)
{
    //when receiving
    if (ICAN::MSG_COMPARE(identifier, ICAN::MSG_ID_t::RELAIS_STATE))
    {
        std::string relaisStateTopic = "canbus/relais_state/0x" + 
            toHexString(ICAN::GET_NOT_MSG(identifier));
        std::string relaisStateData = std::to_string(data[0]);
        m_mqtt.publish(relaisStateTopic.c_str(), relaisStateData.c_str());
        
        std::string custom_string = m_device_list.entry(identifier & 0xFFFFFF00);
        if (custom_string.length() > 0)
        {
            std::string relaisStateTopic_cs = "canbus/relais_state/" + custom_string;
            m_mqtt.publish(relaisStateTopic_cs.c_str(), relaisStateData.c_str());
        }
        
        return true;
    }
    else if (ICAN::MSG_COMPARE(identifier, ICAN::MSG_ID_t::ROLLERSHUTTER_STATE))
    {
        union {
          uint8_t data[sizeof(ICAN::RELAIS_MSG_t)];
          ICAN::RELAIS_MSG_t rm;
        } u;
        
        for (unsigned int i = 0; i < std::min(data_len, sizeof(ICAN::RELAIS_MSG_t)); i++)
        {
            u.data[i] = data[i];
        }
        
        std::string bank_number = "/" + std::to_string(u.rm.bank) + "/" + std::to_string(u.rm.number);
        std::string relaisStateTopic = "canbus/rollershutter_state/0x" + 
            toHexString(ICAN::GET_NOT_MSG(identifier)) + bank_number;
        std::string relaisStateData = std::to_string(u.rm.state);
        m_mqtt.publish(relaisStateTopic.c_str(), relaisStateData.c_str());
        
        std::string custom_string = m_device_list.entry(identifier & 0xFFFFFF00);
        if (custom_string.length() > 0)
        {
            std::string relaisStateTopic_cs = "canbus/rollershutter_state/" + custom_string + bank_number;
            m_mqtt.publish(relaisStateTopic_cs.c_str(), relaisStateData.c_str());
        }
        
        return true;
    }
    return false;
}

void BridgeRelais::dispatch(const char* topic, size_t topic_len, const char* data, size_t data_len)
{
    bool relaiscommand = (strncmp(topic,canbusrelais_topic,sizeof(canbusrelais_topic)-2) == 0);
    bool rollershuttercommand = (strncmp(topic,canbusrollershutter_topic,sizeof(canbusrollershutter_topic)-2) == 0);
    if (relaiscommand || rollershuttercommand)
    {
        uint32_t id = m_device_list.resolve(mqtt_split(topic,topic_len,2));
        union {
            uint8_t data_bytes[8];
            struct {
                uint64_t num : 8;
                uint64_t state : 8;
                uint64_t stop_time : 32;
                uint64_t reserved : 16;
            };
        } du;
        du.num = std::stoi(mqtt_split(data,data_len,0));
        du.state = std::stoi(mqtt_split(data,data_len,1));
        du.stop_time = std::stoi(mqtt_split(data,data_len,2));
        du.reserved = 0;
        if (relaiscommand)
        {
            m_can.send(id + static_cast<uint32_t>(ICAN::MSG_ID_t::RELAIS), du.data_bytes, 8, false);
        }
        else
        {
            m_can.send(id + static_cast<uint32_t>(ICAN::MSG_ID_t::ROLLERSHUTTER), du.data_bytes, 8, false);
        }
    }
}

void BridgeRelais::connected_event()
{
    m_mqtt.subscribe(canbusrelais_topic);
    m_mqtt.subscribe(canbusrollershutter_topic);
}
