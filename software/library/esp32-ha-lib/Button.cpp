#include "Button.hpp"

static void button_dispatch(button_t *b, button_state_t s)
{
    reinterpret_cast<Button*>(b->ctx)->dispatch(s);
}

void Button::dispatch(button_state_t s)
{
    printf("Dispatch Button %d state %d\n", button_data.identifier, s);
    
    switch (s)
    {
        case BUTTON_PRESSED:
            button_data.state = ICAN::BUTTON_EVENT_t::PRESSED;
            button_data.count = 0;
            break;
        case BUTTON_RELEASED:
            // only send a pressed event after a pressed->released
            // that excludes an event on hold->released
            if (button_data.state == ICAN::BUTTON_EVENT_t::PRESSED)
            {
                button_data.state = ICAN::BUTTON_EVENT_t::RELEASED;
                can.send(ICAN::MSG_ID_t::BUTTON_EVENT, data, 2, false);
            }
            button_data.state = ICAN::BUTTON_EVENT_t::RELEASED;
            break;
        case BUTTON_PRESSED_LONG:
            button_data.state = ICAN::BUTTON_EVENT_t::HOLD;
            button_data.count++;
            can.send(ICAN::MSG_ID_t::BUTTON_EVENT, data, 4, false);
            break;
        default:
            break;
    }
}

Button::Button(ICAN& ic, gpio_num_t gpio, button_id_t id) : can(ic)
{
    button.gpio = gpio;
    button.pressed_level = 0;
    button.internal_pull = true;
    button.autorepeat = true;
    button.callback = button_dispatch;
    button.ctx = this;
    button_data.identifier = static_cast<uint8_t>(id);
}

void Button::init()
{
    ESP_ERROR_CHECK(button_init(&button));
}
