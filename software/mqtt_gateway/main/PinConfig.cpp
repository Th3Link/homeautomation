#include "esp32-ha-lib/PinConfig.hpp"
#include <nvs_flash.h>
#include <esp_log.h>

PinConfig::PinConfig()
{
    m_board_config.can_config.rx = GPIO_NUM_14;
    m_board_config.can_config.tx = GPIO_NUM_13;
    m_board_config.onboard_onewire = GPIO_NUM_NC;
    m_board_config.onboard_pwm.dim[0] = GPIO_NUM_NC;
    m_board_config.onboard_pwm.dim[1] = GPIO_NUM_NC;
    m_board_config.onboard_pwm.dim[2] = GPIO_NUM_NC;
    m_board_config.onboard_pwm.dim[3] = GPIO_NUM_NC;
    m_board_config.onboard_pwm.dim[4] = GPIO_NUM_NC;
    m_board_config.onboard_pwm.dim[5] = GPIO_NUM_NC;
    m_board_config.onboard_pwm.dim[6] = GPIO_NUM_NC;
    m_board_config.onboard_pwm.dim[7] = GPIO_NUM_NC;
    
    m_board_config.onboard_relais.scl = GPIO_NUM_NC;
    m_board_config.onboard_relais.sda = GPIO_NUM_NC;
    
    m_board_config.onboard_switch.sw1 = GPIO_NUM_NC;
    m_board_config.onboard_switch.sw2 = GPIO_NUM_NC;
    m_board_config.onboard_switch.sw3 = GPIO_NUM_NC;
    m_board_config.onboard_switch.sw4 = GPIO_NUM_NC;
    
    m_board_config.ext_board_i2c.scl = GPIO_NUM_NC;
    m_board_config.ext_board_i2c.sda = GPIO_NUM_NC;
    m_board_config.ext_board_onewire = GPIO_NUM_NC;
    m_board_config.ext_board_pir = GPIO_NUM_NC;
    m_board_config.ext_board_switch.sw1 = GPIO_NUM_NC;
    m_board_config.ext_board_switch.sw2 = GPIO_NUM_NC;
    m_board_config.ext_board_switch.sw3 = GPIO_NUM_NC;
    m_board_config.ext_board_switch.sw4 = GPIO_NUM_NC;
    
    m_board_config.ext_board_power.button_vcc = GPIO_NUM_NC;
    m_board_config.ext_board_power.button_gnd = GPIO_NUM_NC;
    m_board_config.ext_board_power.sensors_vcc = GPIO_NUM_NC;
    m_board_config.ext_board_power.sensors_gnd = GPIO_NUM_NC;
}

void PinConfig::init()
{
    nvs_handle_t nvs_handle;
    nvs_open("storage", NVS_READWRITE, &nvs_handle);
    uint8_t type = 0;
    if (nvs_get_u8(nvs_handle, "hw_type", &type) != ESP_OK)
    {
        
    }

    uint8_t legacy_sensors = 0;
    nvs_get_u8(nvs_handle, "leg_sen", &legacy_sensors);
    
    uint8_t rev = 0;
    if (nvs_get_u8(nvs_handle, "hw_rev", &rev) != ESP_OK)
    {
        return;
    }
    
    nvs_close(nvs_handle);
    
    switch (static_cast<hw_type_t>(type))
    {
        default:
            break;
    }
    

}

PinConfig::can_config_t PinConfig::get_can_config()
{
    return m_board_config.can_config;
}

gpio_num_t PinConfig::get_ext_board_onewire()
{
    return m_board_config.ext_board_onewire;
}

gpio_num_t PinConfig::get_ext_board_pir()
{
    return m_board_config.ext_board_pir;
}

gpio_num_t PinConfig::get_onboard_onewire()
{
    return m_board_config.onboard_onewire;
}

PinConfig::switch_config_t PinConfig::get_onboard_switch_config()
{
    return m_board_config.onboard_switch;
}

PinConfig::switch_config_t PinConfig::get_ext_board_switch_config()
{
    return m_board_config.ext_board_switch;
}

PinConfig::i2c_config_t PinConfig::get_onboard_relais_config()
{
    return m_board_config.onboard_relais;
}

PinConfig::i2c_config_t PinConfig::get_ext_board_config()
{
    return m_board_config.ext_board_i2c;
}

PinConfig::pwm_config_t PinConfig::get_onboard_pwm_config()
{
    return m_board_config.onboard_pwm;
}

PinConfig::ext_board_t PinConfig::get_ext_board_power()
{
    return m_board_config.ext_board_power;
}
