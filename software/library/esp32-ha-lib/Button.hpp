#pragma once

#include <cstdint>
#include <button.h>
#include "ICAN.hpp"
#include <esp_timer.h>

class Button
{
public:
    enum class button_id_t : uint8_t {
        SW1 = 0,
        SW2 = 1,
        SW3 = 2,
        SW4 = 3,
        EXT_SW1 = 4,
        EXT_SW2 = 5,
        EXT_SW3 = 6,
        EXT_SW4 = 7
    };
    Button(ICAN&, gpio_num_t, button_id_t);
    void init();
    void dispatch(button_state_t);
    void send_multi();
    static const char* TAG;
private:
    ICAN& can;
    button_t button;
    esp_timer_create_args_t timer_args;
    esp_timer_handle_t timer;
    struct button_data_t
    {
        uint8_t identifier;
        ICAN::BUTTON_EVENT_t state;
        uint16_t count;
    };
    #pragma pack(push,1)
    union
    {
        button_data_t button_data;
        uint8_t data[4];
    };
    #pragma pack(pop)
};
