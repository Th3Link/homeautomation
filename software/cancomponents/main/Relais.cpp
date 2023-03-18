#include "Relais.hpp"
#include <freertos/FreeRTOS.h>
#include <freertos/task.h>
#include <i2cdev.h>
#include <pca9557.h>
#include <cstring>
#include <chrono>
#include <esp_err.h>
#include <esp_log.h>
#include <nvs_flash.h>

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
        if (m.data.action >= m_actions[m.data.bank][m.data.number])
        {
            //only execute stop when still the last action
            setRollershutter(m.data.bank, m.data.number, 0, 0);
        }
    }
    else if (m.event == message::Event::BLOCK_TIME)
    {
        setRollershutter(m.data.bank, m.data.number, 3, 0);
        sendRollershutter(m.data.bank, m.data.number);
    }
}

void Relais::receive(message::Message<ICAN::RELAIS_MSG_t>& m)
{
    if (m.event == message::Event::CAN_ROLLERSHUTTER_SET)
    {
        if (!setRollershutter(m.data.bank, m.data.number, m.data.state, m.data.time))
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
        sendRollershutter(m.data.bank, m.data.number);
        return;
    }
    
    if (m.event == message::Event::CAN_RELAIS_SET)
    {
        state(m.data.bank, m.data.number, m.data.state);
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
    relais.bank = m.data.bank;
    relais.number = m.data.number;
    relais.state = state(m.data.bank, m.data.number);
    m_can.send(ICAN::MSG_ID_t::RELAIS_STATE, data8, sizeof(data8), false);
}

void Relais::sendRollershutter(uint8_t p_bank, uint8_t p_number)
{
    bool down = state(p_bank, p_number * 2);
    bool up = state(p_bank, p_number * 2 + 1);
    union {
        ICAN::RELAIS_MSG_t relais;
        uint8_t data8[sizeof(ICAN::RELAIS_MSG_t)];
    };
    relais.bank = p_bank;
    relais.number = p_number;
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
    
    m_can.send(ICAN::MSG_ID_t::ROLLERSHUTTER_STATE, data8, sizeof(data8), false);
}

bool Relais::setRollershutter(uint8_t p_bank, uint8_t p_number, uint8_t p_state, uint32_t p_time)
{

    auto hwoff = !state(p_bank, p_number * 2) && !state(p_bank, p_number * 2 + 1);
    if (p_state == 0)
    {
        
        state(p_bank, p_number * 2, 0); //(0=0,1=2,2=4,3=6,4=8,5=10)
        state(p_bank, p_number * 2 + 1, 0);
        if (m_states[p_bank][p_number] != rollershutter_state_t::STOP)
        {
            m_states[p_bank][p_number] = rollershutter_state_t::BLOCKED;
            message::Message<rollershutter_action_t>::send(m_queue, 
                *this, message::Event::BLOCK_TIME, {p_bank, p_number, 0}, UNBLOCK_TIME);
        }
    }
    else if (p_state == 1 || p_state == 2)
    {
        // go up
        if ((m_states[p_bank][p_number] == rollershutter_state_t::STOP) && hwoff)
        {
            if (m_rollershutter_mode == ICAN::ROLLERSHUTTER_MODE_t::HARDWARE)
            {
                state(p_bank, p_number * 2, 1); //(0=0,1=2,2=4,3=6,4=8,5=10)
                state(p_bank, p_number * 2 + 1, p_state - 1);
            }
            else
            {
                state(p_bank, p_number * 2, 2 - p_state); //(0=0,1=2,2=4,3=6,4=8,5=10)
                state(p_bank, p_number * 2 + 1, p_state - 1);
            }
            
            m_states[p_bank][p_number] = rollershutter_state_t::MOVING;
            m_actions[p_bank][p_number]++;
            if (p_time > 0)
            {
                message::Message<rollershutter_action_t>::send(m_queue, *this, 
                    message::Event::STOP_TIME, {p_bank, p_number, m_actions[p_bank][p_number]}, 
                    std::chrono::milliseconds(p_time));
            }
            sendRollershutter(p_bank, p_number);
        }
        else
        {
            setRollershutter(p_bank, p_number, 0, 0);
            return false;
        }
    }
    else if (p_state == 3)
    {
        // unblock
        if (hwoff)
        {
            m_states[p_bank][p_number] = rollershutter_state_t::STOP;
        }
        else
        {
            state(p_bank, p_number * 2, 0); //(0=0,1=2,2=4,3=6,4=8,5=10)
            state(p_bank, p_number * 2 + 1, 0);
            message::Message<rollershutter_action_t>::send(m_queue, *this, message::Event::BLOCK_TIME, 
                {p_bank, p_number, 0}, UNBLOCK_TIME);
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

Relais::Relais(ICAN& ic) 
    : message::Receiver<ICAN::RELAIS_MSG_t>(m_queue), 
      message::Receiver<rollershutter_action_t>(m_queue), 
      m_can(ic), m_rollershutter_mode(ICAN::ROLLERSHUTTER_MODE_t::SOFTWARE)
{
    for (unsigned int i = 0; i < MAX_RELAIS_COUNT; i++)
    {
        for (unsigned int j = 0; j < 8; j++)
        {
            m_actions[i][j] = 0;
            m_states[i][j] = rollershutter_state_t::STOP;
        }
        memset(&m_device[i][0], 0, sizeof(i2c_dev_t));
        memset(&m_device[i][1], 0, sizeof(i2c_dev_t));
        m_active[i] = false;
        m_state[i] = 0;
        m_relais_mapping[i] = false;
    }
    m_can.add_dispatcher(this);
}

void Relais::init(PinConfig::i2c_config_t onboard_i2c, 
        PinConfig::i2c_config_t ext_board_i2c)
{
    ESP_ERROR_CHECK(pca9557_init_desc(&m_device[0][0], TCA9534_I2C_ADDR_BASE + 0x6,
        onboard_i2c.port, onboard_i2c.sda, onboard_i2c.scl));
    m_device[0][0].cfg.sda_pullup_en = true;
    m_device[0][0].cfg.scl_pullup_en = true;
    ESP_ERROR_CHECK(pca9557_init_desc(&m_device[0][1], TCA9534_I2C_ADDR_BASE + 0x7,
        onboard_i2c.port, onboard_i2c.sda, onboard_i2c.scl));
    m_device[0][1].cfg.sda_pullup_en = true;
    m_device[0][1].cfg.scl_pullup_en = true;
    for (unsigned int i = 1; i < MAX_RELAIS_COUNT; i++)
    {
        uint8_t addr = ((i - 1) << 1);
        ESP_ERROR_CHECK(pca9557_init_desc(&m_device[i][0], TCA9534_I2C_ADDR_BASE + addr,
            ext_board_i2c.port, ext_board_i2c.sda, ext_board_i2c.scl));
        m_device[i][0].cfg.sda_pullup_en = true;
        m_device[i][0].cfg.scl_pullup_en = true;
        ESP_ERROR_CHECK(pca9557_init_desc(&m_device[i][1], TCA9534_I2C_ADDR_BASE + addr + 1,
            ext_board_i2c.port, ext_board_i2c.sda, ext_board_i2c.scl));
        m_device[i][1].cfg.sda_pullup_en = true;
        m_device[i][1].cfg.scl_pullup_en = true;
    }
    
    nvs_handle_t nvs_handle;
    nvs_open("storage", NVS_READONLY, &nvs_handle);
    uint8_t type = 0;
    uint16_t relais_remapping = 0;
    nvs_get_u8(nvs_handle, "can_type", &type);
    nvs_get_u16(nvs_handle, "rremap", &relais_remapping);
    nvs_close(nvs_handle);
    
    // if the device type is not set return here. then all active flage are false
    // and no relais code will be executed
    if (static_cast<ICAN::DEVICE_t>(type) == ICAN::DEVICE_t::Unknown)
    {
        return;
    }
    
    if (static_cast<ICAN::DEVICE_t>(type) == ICAN::DEVICE_t::Rollershutter)
    {
        m_rollershutter_mode = ICAN::ROLLERSHUTTER_MODE_t::HARDWARE;
    }
    
    // read out the mapping. onboard mapping can be derrived from the type
    if (static_cast<ICAN::DEVICE_t>(type) == ICAN::DEVICE_t::Relais ||
        static_cast<ICAN::DEVICE_t>(type) == ICAN::DEVICE_t::Rollershutter)
    {
        m_relais_mapping[0] = true;
    }
    for (unsigned int i = 1; i < MAX_RELAIS_COUNT; i++)
    {
        if (relais_remapping & (1 << (i - 1)))
        {
            m_relais_mapping[i] = true;
        }
    }

    for (unsigned int i = 0; i < MAX_RELAIS_COUNT; i++)
    {
        if (i2c_dev_probe(&m_device[i][0], I2C_DEV_WRITE) == ESP_OK)
        {
            // both io expander must be probed.
            ESP_ERROR_CHECK(i2c_dev_probe(&m_device[i][1], I2C_DEV_WRITE));
            m_active[i] = true;
            ESP_LOGI(TAG, "Probing %d successful", i);
        }
    }
    for (unsigned int i = 0; i < MAX_RELAIS_COUNT; i++)
    {
        if (!m_active[i])
        {
            continue;
        }

        pca9557_port_write(&m_device[i][0], 0);
        pca9557_port_write(&m_device[i][1], 0);
        pca9557_port_set_mode(&m_device[i][0], 0);
        pca9557_port_set_mode(&m_device[i][1], 0);
        pca9557_port_write(&m_device[i][0], 0);
        pca9557_port_write(&m_device[i][1], 0);
    }
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

void Relais::state(uint8_t p_bank, uint8_t p_num, bool p_state)
{
    if (!m_active[p_bank])
    {
        return;
    }
    
    if (m_relais_mapping[p_bank]) {
        p_num = real_num(p_num);
    }
    m_state[p_bank] = (m_state[p_bank] & ~(1 << p_num)) | (p_state << p_num);
    if (p_num < 8)
    {
        ESP_ERROR_CHECK(pca9557_port_write(&m_device[p_bank][0], m_state[p_bank] & 0xFF));
    }
    else
    {
        ESP_ERROR_CHECK(pca9557_port_write(&m_device[p_bank][1], (m_state[p_bank] >> 8)));
    }
}

bool Relais::state(uint8_t p_bank, uint8_t p_num)
{
    if (m_relais_mapping[p_bank]) {
        p_num = real_num(p_num);
    }
    return (m_state[p_bank] & ~(1 << p_num));
}

bool Relais::dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request)
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
        case ICAN::MSG_ID_t::REQUEST_PARAMETER:
        {
            Relais::dispatch((identifier & 0xFFFFFF00) + static_cast<uint8_t>(
                ICAN::MSG_ID_t::ROLLERSHUTTER_MODE), data, data_len, request);
            return false;
        }
        case ICAN::MSG_ID_t::RELAIS:
        {
            // hack to get some data of the message queue, especially for set only commands
            message::Message<ICAN::RELAIS_MSG_t> msg(*this);
            msg.data = relais;
            if (request)
            {
                msg.event = message::Event::CAN_RELAIS_GET;
                
            }
            else
            {
                msg.event = message::Event::CAN_RELAIS_SET;
            }
            receive(msg);
            return true;
        }
        case ICAN::MSG_ID_t::ROLLERSHUTTER:
        {
            message::Message<ICAN::RELAIS_MSG_t> msg(*this);
            msg.data = relais;
            if (request)
            {
                msg.event = message::Event::CAN_ROLLERSHUTTER_GET;               
            }
            else
            {
                msg.event = message::Event::CAN_ROLLERSHUTTER_SET;
            }
            receive(msg);
            return true;
        }
        default:
            break;
    }
    return false;
}

bool Relais::ext_active()
{
    for (unsigned int i = 1; i < MAX_RELAIS_COUNT; i++)
    {
        if (m_active[i])
        {
            return true;
        }
    }
    return false;
}
