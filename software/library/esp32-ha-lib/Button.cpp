#include "Button.hpp"
#include <esp_err.h>
#include <esp_log.h>

const char* Button::TAG = "Button";

static void button_dispatch(button_t *b, button_state_t s)
{
    reinterpret_cast<Button*>(b->ctx)->dispatch(s);
}

static void button_multi_click(void *arg)
{
    reinterpret_cast<Button*>(arg)->send_multi();
}

void Button::dispatch(button_state_t s)
{
    //ESP_LOGI(Button::TAG, "Dispatch Button %d state %d\n", button_data.identifier, s);
    
    switch (s)
    {
        case BUTTON_PRESSED:
            if (button_data.state == ICAN::BUTTON_EVENT_t::RELEASED)
            {
                button_data.state = ICAN::BUTTON_EVENT_t::PRESSED;
                button_data.count = 0;
                can.send(ICAN::MSG_ID_t::BUTTON_EVENT, data, 4, false);
            }
            break;
        case BUTTON_RELEASED:
            // only send a pressed event after a pressed->released
            // that excludes an event on hold->released
            if (button_data.state == ICAN::BUTTON_EVENT_t::PRESSED)
            {
                button_data.state = ICAN::BUTTON_EVENT_t::SINGLE;
                esp_timer_stop(timer);
                ESP_ERROR_CHECK(esp_timer_start_once(timer, 250*1000));
            }
            else if (button_data.state == ICAN::BUTTON_EVENT_t::SINGLE)
            {
                button_data.state = ICAN::BUTTON_EVENT_t::DOUBLE;
                esp_timer_stop(timer);
                ESP_ERROR_CHECK(esp_timer_start_once(timer, 250*1000));
            }
            else if (button_data.state == ICAN::BUTTON_EVENT_t::DOUBLE)
            {
                esp_timer_stop(timer);
                button_data.state = ICAN::BUTTON_EVENT_t::TRIPPLE;
                can.send(ICAN::MSG_ID_t::BUTTON_EVENT, data, 4, false);
                button_data.state = ICAN::BUTTON_EVENT_t::RELEASED;
            }
            else if (button_data.state == ICAN::BUTTON_EVENT_t::HOLD)
            {
                button_data.state = ICAN::BUTTON_EVENT_t::RELEASED;
                can.send(ICAN::MSG_ID_t::BUTTON_EVENT, data, 4, false);
            }
            
            break;
        case BUTTON_PRESSED_LONG:
            // fall trough
        case BUTTON_CLICKED:
            if (button_data.state == ICAN::BUTTON_EVENT_t::PRESSED || 
                button_data.state == ICAN::BUTTON_EVENT_t::HOLD)
            {
                button_data.state = ICAN::BUTTON_EVENT_t::HOLD;
                button_data.count++;
                can.send(ICAN::MSG_ID_t::BUTTON_EVENT, data, 4, false);
            }
            break;
        default:
            break;
    }
}

void Button::send_multi()
{
    can.send(ICAN::MSG_ID_t::BUTTON_EVENT, data, 4, false);
    button_data.state = ICAN::BUTTON_EVENT_t::RELEASED;
}

Button::Button(ICAN& ic) : can(ic), timer(NULL)
{
}

void Button::init(gpio_num_t gpio, button_id_t id)
{
    if (gpio == GPIO_NUM_NC)
    {
        return;
    }

    button.gpio = gpio;
    button.pressed_level = 0;
    button.internal_pull = true;
    button.autorepeat = true;
    button.callback = button_dispatch;
    button.ctx = this;
    button_data.identifier = static_cast<uint8_t>(id);
    button_data.state = ICAN::BUTTON_EVENT_t::RELEASED;
    
    timer_args.arg = this;
    timer_args.name = "button_multi_click";
    timer_args.dispatch_method = ESP_TIMER_TASK;
    timer_args.callback = button_multi_click;
    
    ESP_ERROR_CHECK(button_init(&button));
    ESP_ERROR_CHECK(esp_timer_create(&timer_args, &timer));
    esp_timer_stop(timer);
}
