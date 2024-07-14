#pragma once

#include <freertos/FreeRTOS.h>
#include <freertos/semphr.h>
#include <driver/gpio.h>
#include <vector>
#include "ICAN.hpp"
#include "PinConfig.hpp"
class CAN : public ICAN, public ICANDispatcher
{
    public:      
        CAN();
        void init(PinConfig::can_config_t, bool enable_filter);
        void deinit();
        void send(MSG_ID_t messageId, uint8_t* data, unsigned int data_len, bool request) override;
        void send(uint32_t id, uint8_t* data, unsigned int data_len, bool request) override;
        bool dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request) override;
        void add_dispatcher(ICANDispatcher*) override;
        uint8_t get_id() override;
        uint8_t get_type() override;
        bool enable_filter();
        void bitrate(ICAN::BITRATE_t) override;
        ICAN::BITRATE_t bitrate() override;
        void inc_received();
        void inc_transmitted();
        uint64_t received() override;
        uint64_t transmitted() override;
    
        static const char* TAG;
        bool shutdown_request();
        void shutdown();
    private:
        void read_nvs();
        bool m_enable_filter;
        bool m_update_silence;
        ICAN::BITRATE_t m_bitrate;
        uint8_t m_id;
        uint8_t m_type;
        gpio_num_t m_rx_pin;
        gpio_num_t m_tx_pin;
        std::vector<ICANDispatcher*> m_dispatcher;
        SemaphoreHandle_t m_shutdown_sem;
        bool m_shutdown_request;
        uint64_t m_received;
        uint64_t m_transmitted;
};
