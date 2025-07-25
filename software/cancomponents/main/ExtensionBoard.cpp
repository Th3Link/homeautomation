#include <driver/gpio.h>
#include <esp_err.h>
#include <esp_log.h>

#include "ExtensionBoard.hpp"

const char *ExtensionBoard::TAG = "ExtensionBoard";

ExtensionBoard::ExtensionBoard() {}

void ExtensionBoard::sensor_board_setup(PinConfig::ext_board_t power_config) {
    if (power_config.sensors_vcc == GPIO_NUM_NC || power_config.sensors_gnd == GPIO_NUM_NC) {
        return;
    }

    ESP_LOGI(TAG, "Setup for extension sensor board");
#define PIN_BIT(x) (1ULL << x)
    gpio_config_t io_conf;
    io_conf.intr_type = GPIO_INTR_DISABLE;
    io_conf.mode = GPIO_MODE_OUTPUT;
    io_conf.pin_bit_mask = PIN_BIT(power_config.sensors_vcc) | PIN_BIT(power_config.sensors_gnd);
    io_conf.pull_down_en = GPIO_PULLDOWN_DISABLE;
    io_conf.pull_up_en = GPIO_PULLUP_DISABLE;
    gpio_config(&io_conf);

    gpio_set_level(power_config.sensors_vcc, 1);
    gpio_set_level(power_config.sensors_gnd, 0);
}

void ExtensionBoard::button_board_setup(PinConfig::ext_board_t power_config) {
    if (power_config.button_vcc == GPIO_NUM_NC || power_config.button_gnd == GPIO_NUM_NC) {
        return;
    }

    ESP_LOGI(TAG, "Setup for extension button board");
#define PIN_BIT(x) (1ULL << x)
    gpio_config_t io_conf;
    io_conf.intr_type = GPIO_INTR_DISABLE;
    io_conf.mode = GPIO_MODE_OUTPUT;
    io_conf.pin_bit_mask = PIN_BIT(power_config.button_vcc) | PIN_BIT(power_config.button_gnd);
    io_conf.pull_down_en = GPIO_PULLDOWN_DISABLE;
    io_conf.pull_up_en = GPIO_PULLUP_DISABLE;
    gpio_config(&io_conf);

    gpio_set_level(power_config.button_vcc, 1);
    gpio_set_level(power_config.button_gnd, 0);
}
