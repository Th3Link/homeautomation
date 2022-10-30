#include "BridgeButton.hpp"
#include "helper.hpp"

const char* BridgeButton::TAG = "BridgeButton";

BridgeButton::BridgeButton(ICAN& ic, IMQTT& im) : m_can(ic), m_mqtt(im)
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
        m_mqtt.publish(buttonTopic.c_str(), buttonData.c_str());
        return true;
    }
    return false;
}

void BridgeButton::dispatch(const char* topic, size_t topic_len, const char* data, size_t data_len)
{

}

void BridgeButton::connected_event()
{
    
}
