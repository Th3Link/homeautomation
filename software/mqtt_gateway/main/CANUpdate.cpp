#include "CANUpdate.hpp"

#include <freertos/FreeRTOS.h>
#include <freertos/task.h>
#include <esp_log.h>

#include <algorithm>
#include <string>

const char* CANUpdate::TAG = "CANUpdate";

CANUpdate::CANUpdate(ICAN& ic) : m_can(ic)
{
    
}

void CANUpdate::by_type_start(char* type, uint32_t filesize)
{
    m_update_id = 0x10000000 + ((std::stoul(std::string(type), nullptr, 16) & 0xFF) << 16);
    start(filesize);
}

void CANUpdate::by_uid_start(char* uid, uint32_t filesize)
{
    m_update_id = std::stoul(std::string(uid), nullptr, 16);
    start(filesize);
}

void CANUpdate::start(uint32_t filesize)
{
    m_filesize = filesize;
    //switch to update mode
    uint8_t data[8] {0};
    data[0] = static_cast<uint8_t>(ICAN::AVAILABLE_t::UPDATE_MODE);
    m_can.send(m_update_id + static_cast<uint32_t>(ICAN::MSG_ID_t::RESTART), 
        &data[0], 1, false);
    
    // select flash area
    data[0] = 0;
    data[1] = 0;
    data[2] = 0;
    data[3] = 0;
    data[4] = (filesize >> 24) & 0xFF;
    data[5] = (filesize >> 16) & 0xFF;
    data[6] = (filesize >> 8) & 0xFF;
    data[7] = filesize & 0xFF;
    m_can.send(m_update_id + static_cast<uint32_t>(ICAN::MSG_ID_t::FLASH_SELECT),
        &data[0], 8, false);
    
    // erase flash
    m_can.send(m_update_id + static_cast<uint32_t>(ICAN::MSG_ID_t::FLASH_ERASE), 
        &data[0], 0, false);
}

void CANUpdate::selected_start(char** uids, uint8_t device_count, uint32_t filesize)
{

}

void CANUpdate::abort()
{
    complete();
}

bool CANUpdate::data(char* p_data, uint32_t data_len)
{
    constexpr uint32_t can_max = 8;
    uint32_t remaining = data_len;
    size_t addr = 0;
    while ((remaining > 0) && (m_filesize > 0))
    {
        // slow down transmission. slaves are too slow to compete
        if ((addr % 96) == 0)
        {
            vTaskDelay(pdMS_TO_TICKS(20));
        }
        uint32_t to_send = std::min(std::min(remaining, can_max),m_filesize);
        m_can.send(m_update_id + static_cast<uint32_t>(ICAN::MSG_ID_t::FLASH_WRITE), 
            reinterpret_cast<uint8_t*>(&p_data[addr]), to_send, false);
        remaining -= to_send;
        m_filesize -= to_send;
        addr += to_send;
    }
    return true;
}

void CANUpdate::complete()
{
    uint8_t data[8] {0};
    data[0] = static_cast<uint8_t>(ICAN::AVAILABLE_t::APPLICATION);
    m_can.send(m_update_id + static_cast<uint32_t>(ICAN::MSG_ID_t::RESTART), &data[0], 1, false);
}

