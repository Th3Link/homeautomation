#include <esp_err.h>
#include <esp_log.h>
#include <driver/gpio.h>

#include "ExtensionBoard.hpp"

const char* ExtensionBoard::TAG = "ExtensionBoard";

ExtensionBoard::ExtensionBoard()
{
    
}

void ExtensionBoard::sensor_board_setup(PinConfig::ext_board_t power_config)
{
    ESP_LOGI(TAG, "Setup for extension sensor board");
}
void ExtensionBoard::button_board_setup(PinConfig::ext_board_t power_config)
{
    ESP_LOGI(TAG, "Setup for extension button board");
}
