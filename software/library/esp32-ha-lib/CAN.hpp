#pragma once

#include <freertos/FreeRTOS.h>
#include <freertos/semphr.h>
#include <driver/gpio.h>
#include <vector>
#include "ICAN.hpp"

class CAN : public ICAN
{
    public:      
        CAN(gpio_num_t rx_pin, gpio_num_t tx_pin);
        void init() override;
        void deinit() override;
        void send(MSG_ID_t messageId, uint8_t* data, unsigned int data_len, bool request) override;
        std::vector<ICANDispatcher*> dispatcher();
        void add_dispatcher(ICANDispatcher*) override;
        uint8_t get_id() override;
        uint8_t get_type() override;
        static const char* TAG;
        bool shutdown_request();
        void shutdown();
    private:
        void read_nvs();
        uint8_t m_bitrate;
        uint8_t m_id;
        uint8_t m_type;
        gpio_num_t m_rx_pin;
        gpio_num_t m_tx_pin;
        std::vector<ICANDispatcher*> m_dispatcher;
        static constexpr uint32_t m_can_ng = 0x10000000;
        SemaphoreHandle_t m_shutdown_sem;
        bool m_shutdown_request = false;

};
