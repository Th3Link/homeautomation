#include "Action.hpp"

const char* Actions::TAG = "Actions";

Actions::Actions(ICAN& ic) : m_can(ic)
{
    m_can.add_dispatcher(this);
}

void Actions::dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request)
{
    switch (ICAN::GET_MSG(identifier))
    {
    case ICAN::MSG_ID_t::BUTTON_EVENT:
    {
        auto found = actions.find(
            ICAN::GET_TYPE(identifier) << 24
            | ICAN::GET_ID(identifier) << 16
            | (data[0] << 4 | (data[1] & 0x0F)) << 8);
        if (found != actions.end()) {
            found->second.trigger(0);
        }
        break;
    }
    case ICAN::MSG_ID_t::AMBIENT_LIGHT_SENSOR:
    {
        auto found = actions.find(
            ICAN::GET_TYPE(identifier) << 24
            | ICAN::GET_ID(identifier) << 16;
        if (found != actions.end()) {
            union {
                uint8_t data[4];
                uint32_t data32;
            } t;

            for (auto i = 0; i < std::min(data_len,static_cast<unsigned int>(4)); i++)
            {
                t.data[i] = data[i];
            };
            
            found->second.trigger(t.data32);
        }
        break;
    }
    }
}

void add_combination(uint32_t key, uint8_t trigger_id, Action& action)
{
    actions.insert(std::pair<uint32_t, ActionCombination>(key, ActionCombination(trigger_id, action));
}
