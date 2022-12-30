#include "BridgeButton.hpp"
#include "helper.hpp"

const char* BridgeButton::TAG = "BridgeButton";

BridgeButton::BridgeButton(ICAN& ic, IMQTT& im, DeviceList& dl) : m_can(ic), m_mqtt(im), m_device_list(dl)
{
    m_can.add_dispatcher(this);
    //m_mqtt.add_dispatcher(this);
}

void BridgeButton::init()
{

}

bool BridgeButton::dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request)
{   
    if (ICAN::MSG_COMPARE(identifier, ICAN::MSG_ID_t::BUTTON_EVENT))
    {
        union {
            uint8_t data8[4];
            struct
            {
                uint32_t button_id : 8;
                uint32_t button_event : 8;
                uint32_t count : 16;
            } s;
        } u;

        for (auto i = 0; i < data_len; i++)
        {
            u.data8[i] = data[i];
        };
        
        std::string buttonTopic = "canbus/button/0x" + toHexString(ICAN::GET_NOT_MSG(identifier));
        std::string buttonData = std::to_string(u.s.button_id) + "/";
        
        switch (static_cast<ICAN::BUTTON_EVENT_t>(u.s.button_event))
        {
            case ICAN::BUTTON_EVENT_t::RELEASED:
                buttonData += "released";
                break;
            case ICAN::BUTTON_EVENT_t::HOLD:
                buttonData += "hold";
                break;
            case ICAN::BUTTON_EVENT_t::PRESSED:
                return true;
            case ICAN::BUTTON_EVENT_t::SINGLE:
                buttonData += "single";
                break;
            case ICAN::BUTTON_EVENT_t::DOUBLE:
                buttonData += "double";
                break;
            case ICAN::BUTTON_EVENT_t::TRIPPLE:
                buttonData += "tripple";
                break;
        }

        buttonData += "/" + std::to_string(u.s.count);        

        std::string custom_string = m_device_list.entry(identifier & 0xFFFFFF00);
        if (custom_string.length() > 0)
        {
            std::string buttonTopic_cs = "canbus/button/" + custom_string;
            m_mqtt.publish(buttonTopic_cs.c_str(), buttonData.c_str());
        }
        m_mqtt.publish(buttonTopic.c_str(), buttonData.c_str());
        
        
        return true;
    }
    
    if (ICAN::MSG_COMPARE(identifier, ICAN::MSG_ID_t::PIR_SENSOR))
    {
        union {
            uint8_t data8[4];
            struct
            {
                uint32_t button_id : 8;
                uint32_t button_event : 8;
                uint32_t count : 16;
            } s;
        } u;

        for (auto i = 0; i < data_len; i++)
        {
            u.data8[i] = data[i];
        };
        
        std::string presenceData;
        
        switch (static_cast<ICAN::BUTTON_EVENT_t>(u.s.button_event))
        {
            case ICAN::BUTTON_EVENT_t::RELEASED:
                presenceData += "8/release/";
                break;
            case ICAN::BUTTON_EVENT_t::HOLD:
            case ICAN::BUTTON_EVENT_t::PRESSED:
            case ICAN::BUTTON_EVENT_t::SINGLE:
            case ICAN::BUTTON_EVENT_t::DOUBLE:
            case ICAN::BUTTON_EVENT_t::TRIPPLE:
                presenceData += "8/hold/";
                break;
        }
        
        std::string presenceTopic = "canbus/presence/0x" + toHexString(ICAN::GET_NOT_MSG(identifier));
        presenceData += std::to_string(u.s.count);
        
        std::string custom_string = m_device_list.entry(identifier & 0xFFFFFF00);
        if (custom_string.length() > 0)
        {
            std::string buttonTopic_cs = "canbus/presence/" + custom_string;
            m_mqtt.publish(buttonTopic_cs.c_str(), presenceData.c_str());
        }
        m_mqtt.publish(presenceTopic.c_str(), presenceData.c_str());
    }
    return false;
}

void BridgeButton::dispatch(const char* topic, size_t topic_len, const char* data, size_t data_len)
{

}

void BridgeButton::connected_event()
{
    
}
