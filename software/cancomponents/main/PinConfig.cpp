#include "esp32-ha-lib/PinConfig.hpp"
#include "esp32-ha-lib/ICAN.hpp"
#include <nvs_flash.h>
#include <esp_log.h>

const char* PinConfig::TAG = "PinConfig";

static void set_switch_config(PinConfig::board_config_t& bc, uint8_t rev, uint8_t legacy_sensors)
{
    switch (rev)
    {
        case 1:
            bc.onboard_onewire = GPIO_NUM_4;
            bc.onboard_pwm.dim[0] = GPIO_NUM_21;
            bc.onboard_pwm.dim[1] = GPIO_NUM_19;
            bc.onboard_pwm.dim[2] = GPIO_NUM_25;
            bc.onboard_pwm.dim[3] = GPIO_NUM_27;
            bc.onboard_pwm.dim[4] = GPIO_NUM_23;
            bc.onboard_pwm.dim[5] = GPIO_NUM_22;
            bc.onboard_pwm.dim[6] = GPIO_NUM_32;
            bc.onboard_pwm.dim[7] = GPIO_NUM_26;
            
            bc.onboard_relais.scl = GPIO_NUM_NC;
            bc.onboard_relais.sda = GPIO_NUM_NC;
            
            bc.onboard_switch.sw1 = GPIO_NUM_33;
            bc.onboard_switch.sw2 = GPIO_NUM_35;
            bc.onboard_switch.sw3 = GPIO_NUM_12;
            bc.onboard_switch.sw4 = GPIO_NUM_34;
            
            bc.ext_board_power.button_vcc = GPIO_NUM_5;
            bc.ext_board_power.button_gnd = GPIO_NUM_2;
            bc.ext_board_power.sensors_vcc = GPIO_NUM_5;
            bc.ext_board_power.sensors_gnd = GPIO_NUM_2;
            if (legacy_sensors)
            {
                bc.ext_board_power.sensors_vcc = GPIO_NUM_2;
                bc.ext_board_power.sensors_gnd = GPIO_NUM_18;
            }
            
            bc.ext_board_i2c.scl = GPIO_NUM_15;
            bc.ext_board_i2c.sda = GPIO_NUM_5;
            bc.ext_board_onewire = GPIO_NUM_16;
            bc.ext_board_pir = GPIO_NUM_17;
            bc.ext_board_switch.sw1 = GPIO_NUM_15;
            bc.ext_board_switch.sw2 = GPIO_NUM_16;
            bc.ext_board_switch.sw3 = GPIO_NUM_17;
            bc.ext_board_switch.sw4 = GPIO_NUM_18;
            break;
        case 2:
            bc.onboard_onewire = GPIO_NUM_27;
            bc.onboard_pwm.dim[0] = GPIO_NUM_33;
            bc.onboard_pwm.dim[1] = GPIO_NUM_32;
            bc.onboard_pwm.dim[2] = GPIO_NUM_21;
            bc.onboard_pwm.dim[3] = GPIO_NUM_23;
            bc.onboard_pwm.dim[4] = GPIO_NUM_19;
            bc.onboard_pwm.dim[5] = GPIO_NUM_22;
            bc.onboard_pwm.dim[6] = GPIO_NUM_NC;
            bc.onboard_pwm.dim[7] = GPIO_NUM_NC;
            
            bc.onboard_relais.scl = GPIO_NUM_NC;
            bc.onboard_relais.sda = GPIO_NUM_NC;
            
            bc.onboard_switch.sw1 = GPIO_NUM_25;
            bc.onboard_switch.sw2 = GPIO_NUM_26;
            bc.onboard_switch.sw3 = GPIO_NUM_5;
            bc.onboard_switch.sw4 = GPIO_NUM_15;
            
            bc.ext_board_i2c.scl = GPIO_NUM_16;
            bc.ext_board_i2c.sda = GPIO_NUM_4;
            bc.ext_board_onewire = GPIO_NUM_17;
            bc.ext_board_pir = GPIO_NUM_18;
            bc.ext_board_switch.sw1 = GPIO_NUM_4;
            bc.ext_board_switch.sw2 = GPIO_NUM_16;
            bc.ext_board_switch.sw3 = GPIO_NUM_17;
            bc.ext_board_switch.sw4 = GPIO_NUM_18;
            break;
        default:
            break;
    }
}

static void set_relais_config(PinConfig::board_config_t& bc, uint8_t rev, uint8_t legacy_sensors)
{
    switch (rev)
    {
        case 1:
            bc.onboard_onewire = GPIO_NUM_4;
            
            bc.onboard_relais.scl = GPIO_NUM_19;
            bc.onboard_relais.sda = GPIO_NUM_21;
            
            bc.ext_board_power.button_vcc = GPIO_NUM_5;
            bc.ext_board_power.button_gnd = GPIO_NUM_2;
            bc.ext_board_power.sensors_vcc = GPIO_NUM_5;
            bc.ext_board_power.sensors_gnd = GPIO_NUM_2;
            if (legacy_sensors)
            {
                bc.ext_board_power.sensors_vcc = GPIO_NUM_2;
                bc.ext_board_power.sensors_gnd = GPIO_NUM_18;
            }
            
            bc.ext_board_i2c.scl = GPIO_NUM_15;
            bc.ext_board_i2c.sda = GPIO_NUM_5;
            bc.ext_board_onewire = GPIO_NUM_16;
            bc.ext_board_pir = GPIO_NUM_17;
            bc.ext_board_switch.sw1 = GPIO_NUM_15;
            bc.ext_board_switch.sw2 = GPIO_NUM_16;
            bc.ext_board_switch.sw3 = GPIO_NUM_17;
            bc.ext_board_switch.sw4 = GPIO_NUM_18;
            break;
        case 2:
            bc.onboard_onewire = GPIO_NUM_4;
            
            bc.onboard_relais.scl = GPIO_NUM_19;
            bc.onboard_relais.sda = GPIO_NUM_21;
                        
            bc.ext_board_i2c.scl = GPIO_NUM_15;
            bc.ext_board_i2c.sda = GPIO_NUM_16;
            bc.ext_board_onewire = GPIO_NUM_17;
            bc.ext_board_pir = GPIO_NUM_18;
            bc.ext_board_switch.sw1 = GPIO_NUM_15;
            bc.ext_board_switch.sw2 = GPIO_NUM_16;
            bc.ext_board_switch.sw3 = GPIO_NUM_17;
            bc.ext_board_switch.sw4 = GPIO_NUM_18;
            break;
        default:
            break;
    }
}

PinConfig::PinConfig()
{
    m_board_config.can_config.tx = GPIO_NUM_13;
    m_board_config.can_config.rx = GPIO_NUM_14;
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
    m_board_config.onboard_relais.port = I2C_NUM_0;
    
    m_board_config.onboard_switch.sw1 = GPIO_NUM_NC;
    m_board_config.onboard_switch.sw2 = GPIO_NUM_NC;
    m_board_config.onboard_switch.sw3 = GPIO_NUM_NC;
    m_board_config.onboard_switch.sw4 = GPIO_NUM_NC;
    
    m_board_config.ext_board_i2c.scl = GPIO_NUM_NC;
    m_board_config.ext_board_i2c.sda = GPIO_NUM_NC;
    m_board_config.ext_board_i2c.port = I2C_NUM_1;
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
    //Initialize NVS
    esp_err_t ret = nvs_flash_init();
    if (ret == ESP_ERR_NVS_NO_FREE_PAGES || ret == ESP_ERR_NVS_NEW_VERSION_FOUND) {
      ESP_ERROR_CHECK(nvs_flash_erase());
      ret = nvs_flash_init();
    }
    ESP_ERROR_CHECK(ret);
    
    nvs_handle_t nvs_handle;
    nvs_open("storage", NVS_READWRITE, &nvs_handle);
    uint8_t type = 0;
    if (nvs_get_u8(nvs_handle, "can_type", &type) != ESP_OK)
    {
        ESP_LOGE(TAG, "Device type not set");
    }

    uint8_t legacy_sensors = 0;
    nvs_get_u8(nvs_handle, "leg_sen", &legacy_sensors);
    
    uint8_t rev = 0;
    if (nvs_get_u8(nvs_handle, "hw_rev", &rev) != ESP_OK)
    {
        ESP_LOGE(TAG, "Hardware Revision not set");
        return;
    }
    
    nvs_close(nvs_handle);
    
    switch (static_cast<ICAN::DEVICE_t>(type))
    {
        case ICAN::DEVICE_t::Button:
            ESP_LOGI(TAG, "Configure Buttons");
            set_switch_config(m_board_config, rev, legacy_sensors);
            break;
        case ICAN::DEVICE_t::Rollershutter:
            // fall through
        case ICAN::DEVICE_t::Relais:
            // fall through
        case ICAN::DEVICE_t::SSR:
            ESP_LOGI(TAG, "Configure Relais");
            set_relais_config(m_board_config, rev, legacy_sensors);
            break;
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
