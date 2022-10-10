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
    #define PIN_BIT(x) (1ULL<<x)
	gpio_config_t io_conf;
	io_conf.intr_type = GPIO_INTR_DISABLE;
	io_conf.mode = GPIO_MODE_OUTPUT;
	io_conf.pin_bit_mask = PIN_BIT(EXT_SENSOR_VCC) | PIN_BIT(EXT_SENSOR_GND);
	io_conf.pull_down_en = GPIO_PULLDOWN_DISABLE;
	io_conf.pull_up_en = GPIO_PULLUP_DISABLE;
	gpio_config(&io_conf);
    
    gpio_set_level(EXT_SENSOR_VCC, 1);
    gpio_set_level(EXT_SENSOR_GND, 0);
}
void ExtensionBoard::button_board_setup()
{
    ESP_LOGI(TAG, "Setup for extension button board");
    #define PIN_BIT(x) (1ULL<<x)
	gpio_config_t io_conf;
	io_conf.intr_type = GPIO_INTR_DISABLE;
	io_conf.mode = GPIO_MODE_OUTPUT;
	io_conf.pin_bit_mask = PIN_BIT(EXT_BUTTON_VCC) | PIN_BIT(EXT_BUTTON_GND);
	io_conf.pull_down_en = GPIO_PULLDOWN_DISABLE;
	io_conf.pull_up_en = GPIO_PULLUP_DISABLE;
	gpio_config(&io_conf);
    
    gpio_set_level(EXT_BUTTON_VCC, 1);
    gpio_set_level(EXT_BUTTON_GND, 0);
}
