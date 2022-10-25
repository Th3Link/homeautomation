#pragma once

#include <cstdint>
#include "IUpdate.hpp"
#include "esp32-ha-lib/ICAN.hpp"

class CANUpdate : public IUpdate
{
public:
    CANUpdate(ICAN&);
    
    void start(uint32_t filesize);
    void by_type_start(char* type, uint32_t filesize);
    void by_uid_start(char* uid, uint32_t filesize);
    void selected_start(char** uids, uint8_t device_count, uint32_t filesize);
    void abort() override;
    bool data(char* data, uint32_t data_len) override;
    void complete() override;
private:
    ICAN& m_can;
    uint32_t m_update_id;
    uint32_t m_filesize;
    static const char* TAG;
};
