#include "Relais.hpp"
#include <freertos/FreeRTOS.h>
#include <freertos/task.h>
#include <i2cdev.h>
#include "pca9534.h"
#include <cstring>
#include <chrono>
#include <esp_err.h>
#include <esp_log.h>

#include <message/Atomic.hpp>
#include <message/Receiver.hpp>
#include <message/Message.hpp>

const char* Relais::TAG = "Relais";
constexpr uint8_t relais1_address = 0x26; //0b0010 0110
constexpr uint8_t relais2_address = 0x27; //0b0010 0111
void message::atomic(bool)
{
    
}

/*
 * How do we encode switch on and off and a time?
 * 8 bytes
 */
constexpr auto UNBLOCK_TIME = std::chrono::milliseconds(500);
constexpr auto REQUEUE_TIME = std::chrono::milliseconds(600);

void Relais::receive(message::Message<rollershutter_action_t>& m)
{
    if (m.event == message::Event::STOP_TIME)
    {
        if (m.data.action >= m_actions[m.data.number])
        {
            //only execute stop when still the last action
            setRollershutter(m.data.number, 0, 0);
        }
    }
    else if (m.event == message::Event::BLOCK_TIME)
    {
        setRollershutter(m.data.number, 3, 0);
        sendRollershutter(m.data.number);
    }
}

void Relais::receive(message::Message<ICAN::RELAIS_MSG_t>& m)
{
    if (m.event == message::Event::CAN_ROLLERSHUTTER_SET)
    {
        if (!setRollershutter(m.data.number, m.data.state, m.data.time))
        {
            //requeue
            message::Message<ICAN::RELAIS_MSG_t>::send(m_queue, *this, 
                message::Event::CAN_ROLLERSHUTTER_SET, 
                std::move(m.data), REQUEUE_TIME);
            return;
        }
        return;
        
    }
    else if (m.event == message::Event::CAN_ROLLERSHUTTER_GET)
    {
        sendRollershutter(m.data.number);
        return;
    }
    
    if (m.event == message::Event::CAN_RELAIS_SET)
    {
        state(m.data.number, m.data.state);
        /*
         * invert switch after timeout time 
         */
        if (m.data.time > 0)
        {
            auto time = m.data.time;
            ESP_LOGI(Relais::TAG, "Relais time: %lu\n", time);
            m.data.state = !m.data.state;
            m.data.time  = 0;
            
            // queue timeout message in
            message::Message<ICAN::RELAIS_MSG_t>::send(m_queue, *this, 
                message::Event::CAN_RELAIS_SET, 
                std::move(m.data), std::chrono::milliseconds(time));
        }
    }
    
    // send state notification anyway
    union {
        ICAN::RELAIS_MSG_t relais;
        uint8_t data8[sizeof(ICAN::RELAIS_MSG_t)];
    };
    relais.time = 0;
    relais.number = m.data.number;
    relais.state = state(m.data.number);
    m_can.send(ICAN::MSG_ID_t::RELAIS, data8, sizeof(data8), false);
}

void Relais::sendRollershutter(uint8_t number)
{
    bool down = state(number * 2);
    bool up = state(number * 2 + 1);
    union {
        ICAN::RELAIS_MSG_t relais;
        uint8_t data8[sizeof(ICAN::RELAIS_MSG_t)];
    };
    relais.number = number;
    relais.time = 0;
    relais.state = 0;
    
    if (down && !up)
    {
        relais.state = 1;
    }
    else if (!down && up)
    {
        relais.state = 2;
    }
    else if (up && down)
    {
        relais.state = 0xFF;
    }
    
    m_can.send(ICAN::MSG_ID_t::RELAIS, data8, sizeof(data8), false);
}

bool Relais::setRollershutter(uint8_t p_number, uint8_t p_state, uint32_t p_time)
{

    auto hwoff = !state(p_number * 2) && !state(p_number * 2 + 1);
    if (p_state == 0)
    {
        
        state(p_number * 2, 0); //(0=0,1=2,2=4,3=6,4=8,5=10)
        state(p_number * 2 + 1, 0);
        if (m_states[p_number] != rollershutter_state_t::STOP)
        {
            m_states[p_number] = rollershutter_state_t::BLOCKED;
            message::Message<rollershutter_action_t>::send(m_queue, 
                *this, message::Event::BLOCK_TIME, {p_number, 0}, UNBLOCK_TIME);
        }
    }
    else if (p_state == 1 || p_state == 2)
    {
        // go up
        if ((m_states[p_number] == rollershutter_state_t::STOP) && hwoff && p_time > 0)
        {
            state(p_number * 2, 2 - p_state); //(0=0,1=2,2=4,3=6,4=8,5=10)
            state(p_number * 2 + 1, p_state - 1);
            m_states[p_number] = rollershutter_state_t::MOVING;
            m_actions[p_number]++;
            message::Message<rollershutter_action_t>::send(m_queue, *this, 
                message::Event::STOP_TIME, {p_number, m_actions[p_number]}, 
                std::chrono::milliseconds(p_time));
            sendRollershutter(p_number);
        }
        else
        {
            setRollershutter(p_number, 0, 0);
            return false;
        }
    }
    else if (p_state == 3)
    {
        // unblock
        if (hwoff)
        {
            m_states[p_number] = rollershutter_state_t::STOP;
        }
        else
        {
            state(p_number * 2, 0); //(0=0,1=2,2=4,3=6,4=8,5=10)
            state(p_number * 2 + 1, 0);
            message::Message<rollershutter_action_t>::send(m_queue, *this, message::Event::BLOCK_TIME, 
                {p_number, 0}, UNBLOCK_TIME);
        }
    }
    return true;
}

static void message_task(void *this_ptr)
{
    auto& r = *reinterpret_cast<Relais*>(this_ptr);
    
    while (1)
    {
        r.m_queue.dispatch();
        vTaskDelay(pdMS_TO_TICKS(100));
    }
}

Relais::Relais(gpio_num_t sda_pin, gpio_num_t scl_pin, ICAN& ic) 
    : message::Receiver<ICAN::RELAIS_MSG_t>(m_queue), 
      message::Receiver<rollershutter_action_t>(m_queue), 
      m_can(ic), m_state(0), m_states({rollershutter_state_t::STOP}),
      m_actions({0})
{
    m_can.add_dispatcher(this);
    memset(&m_device, 0, sizeof(i2c_dev_t));
    ESP_ERROR_CHECK(pca9534_init_desc(&m_device, 0, sda_pin, scl_pin));
}

void Relais::init()
{
    ESP_ERROR_CHECK(i2cdev_init());

    pca9534_port_set_mode(&m_device, relais1_address,0);
    pca9534_port_set_mode(&m_device, relais2_address,0);
    pca9534_port_write(&m_device, relais1_address, 0);
    pca9534_port_write(&m_device, relais2_address, 0);
    xTaskCreate(message_task, "message_task", configMINIMAL_STACK_SIZE * 3, this, 5, NULL);
}

uint8_t real_num(uint8_t num)
{
    switch (num)
    {
        case 0:
            return 3;
        case 1:
            return 2;
        case 2:
            return 1;
        case 3:
            return 7;
        case 4:
            return 6;
        case 5:
            return 5;
        case 6:
            return 4;
        case 7:
            return 3+8;
        case 8:
            return 2+8;
        case 9:
            return 1+8;
        case 10:
            return 7+8;
        case 11:
            return 6+8;
    }
    return 0;
}

void Relais::state(uint8_t num, bool state)
{
    num = real_num(num);
    m_state = (m_state & ~(1<<num)) | state<<num;
    if (num < 8)
    {
        ESP_ERROR_CHECK(pca9534_port_write(&m_device, relais1_address, m_state & 0xFF));
    }
    else
    {
        ESP_ERROR_CHECK(pca9534_port_write(&m_device, relais2_address, (m_state >> 8)));
    }
}

bool Relais::state(uint8_t num)
{
    num = real_num(num);
    return (m_state & ~(1<<num));
}

void Relais::dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request)
{
    union {
        ICAN::RELAIS_MSG_t relais;
        uint8_t data8[sizeof(ICAN::RELAIS_MSG_t)];
    };
    
    for(auto& d : data8)
    {
        d = 0;
    }
    for (unsigned int i = 0; i < std::min(data_len, sizeof(ICAN::RELAIS_MSG_t)); i++)
    {
        data8[i] = data[i];
    }
    
    switch (static_cast<ICAN::MSG_ID_t>(identifier & 0xFF))
    {
        case ICAN::MSG_ID_t::RELAIS:
        {
            if (request)
            {
                message::Message<ICAN::RELAIS_MSG_t>::send(m_queue, *this, 
                    message::Event::CAN_RELAIS_GET, std::move(relais));                
            }
            else
            {
                message::Message<ICAN::RELAIS_MSG_t>::send(m_queue, *this, 
                    message::Event::CAN_RELAIS_SET, std::move(relais));
            }

            break;
        }
        case ICAN::MSG_ID_t::ROLLERSHUTTER:
        {
            if (request)
            {
                message::Message<ICAN::RELAIS_MSG_t>::send(m_queue, *this, 
                    message::Event::CAN_ROLLERSHUTTER_GET, std::move(relais));                
            }
            else
            {
                message::Message<ICAN::RELAIS_MSG_t>::send(m_queue, *this, 
                    message::Event::CAN_ROLLERSHUTTER_SET, std::move(relais));
            }
            break;
        }
        default:
            break;
    }
}
