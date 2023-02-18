#include <esp_err.h>
#include <esp_log.h>
#include <driver/gpio.h>

#include "ExtensionBoard.hpp"
#include "gpio_definition.hpp"

const char* ExtensionBoard::TAG = "ExtensionBoard";

ExtensionBoard::ExtensionBoard()
{
    
}

void ExtensionBoard::sensor_board_setup()
{
    ESP_LOGI(TAG, "Setup for extension sensor board");
}
void ExtensionBoard::button_board_setup()
{
    ESP_LOGI(TAG, "Setup for extension button board");
}
