#pragma once

#include "cJSON.h"
#include "esp32-ha-lib/ICAN.hpp"
#include <array>
#include <cstdint>
#include <string>

class DeviceList : public ICANDispatcher {
  public:
    DeviceList(ICAN &);
    bool dispatch(uint32_t identifier, uint8_t *data, unsigned int data_len, bool request) override;
    void init();
    void output(cJSON *object);
    std::string entry(uint32_t id);
    uint32_t resolve(std::string);
    struct DeviceListEntry {
        uint32_t id;
        std::array<char, 10> custom_string;
        std::array<char, 12> version;
        uint32_t last_seen;
        uint32_t uptime;
        uint8_t baudrate;
        uint8_t hwrev;
        uint8_t extension_mode;
        std::array<uint8_t, 8> uid0;
        std::array<uint8_t, 8> uid1;
        uint8_t relais_mode;
        uint8_t state;
        uint8_t error;
    };
    static const char *TAG;

  private:
    ICAN &m_can;
    static constexpr size_t DEVICE_LIST_SIZE = 100;
    DeviceListEntry m_deviceList[DEVICE_LIST_SIZE];
};
