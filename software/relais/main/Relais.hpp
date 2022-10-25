#pragma once

#include <driver/gpio.h>
#include <i2cdev.h>
#include <message/Receiver.hpp>
#include <message/Message.hpp>
#include <message/Queue.hpp>

#include <array>
#include "esp32-ha-lib/ICAN.hpp"

enum class rollershutter_state_t {
    MOVING, STOP, BLOCKED
};
struct rollershutter_action_t
{
    uint8_t number;
    uint32_t action;
};

class Relais : public ICANDispatcher, public message::Receiver<ICAN::RELAIS_MSG_t>, 
    public message::Receiver<rollershutter_action_t>
{
public:
    Relais(gpio_num_t sda_pin, gpio_num_t scl_pin, ICAN&);
    void init();
    void state(uint8_t num, bool state);
    bool state(uint8_t num);
    bool dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request) override;
    void receive(message::Message<ICAN::RELAIS_MSG_t>&) override;
    void receive(message::Message<rollershutter_action_t>&) override;
    static const char* TAG;
    message::Queue<80,80> m_queue;
private:
    
    bool setRollershutter(uint8_t number, uint8_t state, uint32_t time);
    void sendRollershutter(uint8_t number);
    
    ICAN& m_can;
    i2c_dev_t m_device;
    uint16_t m_state;
    ICAN::ROLLERSHUTTER_MODE_t m_rollershutter_mode;
    std::array<rollershutter_state_t, 6> m_states;
    std::array<uint32_t, 6> m_actions;
};
