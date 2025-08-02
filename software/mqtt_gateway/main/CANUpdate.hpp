#pragma once

#include "IUpdate.hpp"
#include "esp32-ha-lib/ICAN.hpp"
#include <cstdint>

class CANUpdate : public IUpdate {
  public:
    CANUpdate(ICAN &);
    void init();
    const char *update_delay();
    void update_delay(const char *c);
    void start(uint32_t filesize, uint32_t crc);
    void by_type_start(char *type, uint32_t filesize, uint32_t crc);
    void by_uid_start(char *uid, uint32_t filesize, uint32_t crc);
    void selected_start(char **uids, uint8_t device_count, uint32_t filesize);
    void abort() override;
    bool data(char *data, uint32_t data_len) override;
    void complete() override;

  private:
    ICAN &m_can;
    uint32_t m_update_id;
    uint32_t m_filesize;
    uint32_t m_update_delay;
    static const char *TAG;
    static constexpr uint32_t UPDATE_DELAY_DEFAULT = 10;
};
