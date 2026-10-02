#pragma once

#include <cstdint>
#include <driver/gpio.h>
#include <driver/i2c.h>

class PinConfig
{
public:   
    struct ext_board_t
    {
        gpio_num_t button_vcc;
        gpio_num_t button_gnd;
        gpio_num_t sensors_vcc;
        gpio_num_t sensors_gnd;
    };
    struct can_config_t
    {
        gpio_num_t tx;
        gpio_num_t rx;
    };
    struct i2c_config_t
    {
        gpio_num_t sda;
        gpio_num_t scl;
        i2c_port_t port;
    };
    struct switch_config_t
    {
        gpio_num_t sw1;
        gpio_num_t sw2;
        gpio_num_t sw3;
        gpio_num_t sw4;
    };
    struct pwm_config_t
    {
        gpio_num_t dim[8];
    };

    struct board_config_t
    {
        can_config_t can_config;
        i2c_config_t onboard_relais;
        i2c_config_t ext_board_i2c;
        gpio_num_t onboard_onewire;
        gpio_num_t ext_board_onewire;
        gpio_num_t ext_board_pir;
        switch_config_t onboard_switch;
        switch_config_t ext_board_switch;
        pwm_config_t onboard_pwm;
        ext_board_t ext_board_power;
    };

    PinConfig();
    void init();
    can_config_t get_can_config();
    gpio_num_t get_onboard_onewire();
    gpio_num_t get_ext_board_onewire();
    gpio_num_t get_ext_board_pir();
    switch_config_t get_onboard_switch_config();
    switch_config_t get_ext_board_switch_config();
    i2c_config_t get_onboard_relais_config();
    i2c_config_t get_ext_board_config();
    pwm_config_t get_onboard_pwm_config();
    ext_board_t get_ext_board_power();
    static const char* TAG;
private:
    board_config_t m_board_config;
};
