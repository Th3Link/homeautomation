#pragma once

#include <unordered_map>
#include "esp32-ha-lib/ICAN.hpp"

class Action
{
public:
    Action();
    void trigger(uint8_t trigger_id, uint32_t value);
};

class ActionCombination
{
public:
    ActionCombination(uint8_t trigger_id, Action& action) : m_trigger_id(trigger_id), m_action(action)
    {
    }
    void trigger(uint32_t value)
    {
        m_action.trigger(m_trigger_id, value);
    }
private:
    uint8_t m_trigger_id
    Action& m_action
};

class Actions : public ICANDispatcher
{
public:
    Actions(ICAN&);
    bool dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request) override;
    void add_combination(uint8_t trigger_id, Action& action);
private:
    ICAN& m_can;
    std::unordered_map<uint32_t, ActionCombination> actions;
    static const char* TAG;
};
