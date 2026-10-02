#pragma once

#include "esp32-ha-lib/PinConfig.hpp"

class ExtensionBoard
{
public:
    ExtensionBoard();
    void sensor_board_setup(PinConfig::ext_board_t);
    void button_board_setup(PinConfig::ext_board_t);
    static const char* TAG;
};
