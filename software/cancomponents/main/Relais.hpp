#pragma once

#include <driver/gpio.h>
#include <i2cdev.h>
#include <message/Receiver.hpp>
#include <message/Message.hpp>
#include <message/Queue.hpp>
#include "esp32-ha-lib/PinConfig.hpp"
#include <array>
#include "esp32-ha-lib/ICAN.hpp"

enum class rollershutter_state_t {
    MOVING_UP, MOVING_DOWN, STOP, BLOCKED
};
struct rollershutter_action_t
{
    uint8_t bank;
    uint8_t number;
    uint32_t action;
};

class Relais : public ICANDispatcher, public message::Receiver<ICAN::RELAIS_MSG_t>, 
    public message::Receiver<rollershutter_action_t>
{
public:
    Relais(ICAN&);
    void init(PinConfig::i2c_config_t, PinConfig::i2c_config_t);
    void state(uint8_t p_bank, uint8_t p_num, bool p_state);
    bool state(uint8_t p_bank, uint8_t p_num);
    bool dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request) override;
    void receive(message::Message<ICAN::RELAIS_MSG_t>&) override;
    void receive(message::Message<rollershutter_action_t>&) override;
    bool ext_active();
    static const char* TAG;
    message::Queue<256,256> m_queue;
    static constexpr uint8_t MAX_RELAIS_COUNT = 9;
    static constexpr uint8_t MAX_CHANNEL_COUNT = 16;
private:
    
    bool setRollershutter(uint8_t bank, uint8_t number, uint8_t state, uint32_t time);
    void sendRollershutter(uint8_t bank, uint8_t number);
    
    ICAN& m_can;
    i2c_dev_t m_device[MAX_RELAIS_COUNT][2];
    std::array<uint16_t, MAX_RELAIS_COUNT> m_state;
    ICAN::ROLLERSHUTTER_MODE_t m_rollershutter_mode;
    std::array<std::array<rollershutter_state_t, MAX_CHANNEL_COUNT/2>,MAX_RELAIS_COUNT> m_states;
    std::array<std::array<uint32_t, MAX_CHANNEL_COUNT/2>,MAX_RELAIS_COUNT> m_actions;
    std::array<std::array<bool, 2>, MAX_RELAIS_COUNT> m_active;
    std::array<bool, MAX_RELAIS_COUNT> m_relais_mapping;
};
