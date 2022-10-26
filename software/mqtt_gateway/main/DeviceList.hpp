#pragma once

#include <cstdint>
#include <array>
#include "cJSON.h"
#include "esp32-ha-lib/ICAN.hpp"

class DeviceList : public ICANDispatcher
{
public:
    DeviceList(ICAN&);
    bool dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request) override;
    void refresh();
    void init();
    void output(cJSON* object);
    
    struct DeviceListEntry
    {
        uint32_t id;
        std::array<char, 8> custom_string;
        std::array<char, 12> version;
        uint32_t last_seen;
        uint32_t uptime;
        uint8_t baudrate;
        std::array<uint8_t, 8> uid0;
        std::array<uint8_t, 8> uid1;
        uint8_t rollershutter_mode;
        uint8_t state;
        uint8_t error;
    };

private:
    ICAN& m_can;
    static constexpr size_t DEVICE_LIST_SIZE = 50;
    DeviceListEntry m_deviceList[DEVICE_LIST_SIZE];
    static const char* TAG;
};
