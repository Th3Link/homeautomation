#pragma once

#include <cstdint>
#include "IMQTT.hpp"
#include "esp32-ha-lib/CAN.hpp"
#include "esp32-ha-lib/PinConfig.hpp"

class Logging : public CAN
{
public:
    /*enum class OUTPUT_t : uint8_t
    {
        MQTT,
        WEB
    };*/
    Logging(PinConfig::can_config_t can_config, bool enable_filter, IMQTT&);
    void mqtt_log(uint32_t id, uint8_t* data, unsigned int data_len, bool request);
    void mqtt_logging(bool);
    bool mqtt_logging();
    //void can_log(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request);
    //void options();
    void init() override;
    void send(uint32_t id, uint8_t* data, unsigned int data_len, bool request) override;
    bool dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request) override;
private:
    static const char* TAG;
    IMQTT& m_mqtt;
    bool m_mqtt_logging;
};
